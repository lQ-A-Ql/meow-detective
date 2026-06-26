use super::error::ParallelEnumError;
use super::partition_worker::PartitionWork;
use crate::staging;
#[cfg(test)]
use domain::{DataSourceId, EntryType, FileEntry, FileEntryId};
use evidence_core::{EvidenceReader, RawImageReader};
use fs_ntfs::mft_scanner::{MftRecord, MftScanner};
use fs_ntfs::{NtfsDirectoryEntry, NtfsReader};
use image_e01::E01Reader;
#[cfg(test)]
use persistence_sqlite::repositories::file_repo::FileRepo;
use rusqlite::params;
use std::collections::{HashMap, HashSet, VecDeque};
use std::io::{Read, Seek, SeekFrom};
use std::sync::atomic::{AtomicBool, Ordering};

const MFT_CHUNK_RECORDS: u64 = 10_000;
const MFT_FALLBACK_SIZE: u64 = 100 * 1024 * 1024;

pub(super) fn enumerate_ntfs_mft_to_staging(
    conn: &rusqlite::Connection,
    partition: &PartitionWork,
    ds_id: &str,
    cancel_token: &AtomicBool,
    progress_cb: Option<&dyn Fn(u64, u64)>,
) -> Result<(u64, u64, u64), ParallelEnumError> {
    if cancel_token.load(Ordering::Relaxed) {
        return Err(ParallelEnumError::Cancelled);
    }

    let mut reader = open_partition_evidence_reader(partition)?;
    let params = read_ntfs_mft_parameters(partition, &mut *reader)?;
    if params.mft_data_size == 0 {
        return Err(ParallelEnumError::MftParams(
            "MFT data size is zero".to_string(),
        ));
    }
    let scanner = MftScanner::new(
        params.volume_offset,
        params.mft_cluster,
        params.cluster_size,
        params.record_size,
        params.bytes_per_sector,
        params.mft_data_size,
    );
    let total_records = scanner.total_records();
    if total_records == 0 {
        return Err(ParallelEnumError::MftParams(
            "MFT total record count is zero".to_string(),
        ));
    }

    conn.execute_batch("BEGIN TRANSACTION")
        .map_err(ParallelEnumError::Db)?;
    let transaction_result = (|| {
        let mut stmt = conn
            .prepare_cached(
                "INSERT OR IGNORE INTO file_entries
                 (id, parent_id, data_source_id, path, name, entry_type,
                  size, ext, deleted, hidden, system, created_at, modified_at, accessed_at, changed_at, hash_sha256)
                 VALUES (?1, ?2, ?3, '', ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, NULL)",
            )
            .map_err(ParallelEnumError::Db)?;
        let mut start_record = 0u64;
        let mut file_count = 0u64;
        let mut dir_count = 0u64;
        let mut total_size = 0u64;
        let mut path_map: HashMap<String, (Option<String>, String, bool)> = HashMap::new();
        let mut deleted_records: HashSet<String> = HashSet::new();
        let mut buf = Vec::new();

        while start_record < total_records {
            if cancel_token.load(Ordering::Relaxed) {
                return Err(ParallelEnumError::Cancelled);
            }

            let chunk_count = MFT_CHUNK_RECORDS.min(total_records - start_record);
            let byte_count = chunk_count * scanner.record_size() as u64;
            buf.resize(byte_count as usize, 0);
            let mft_stream_offset = start_record * scanner.record_size() as u64;
            if params.mft_data_runs.is_empty() {
                let byte_offset = scanner.mft_abs_offset() + mft_stream_offset;
                reader
                    .seek(SeekFrom::Start(byte_offset))
                    .map_err(ParallelEnumError::Io)?;
                reader
                    .read_exact(&mut buf[..byte_count as usize])
                    .map_err(ParallelEnumError::Io)?;
            } else {
                read_ntfs_mft_stream(
                    &mut *reader,
                    params.volume_offset,
                    params.cluster_size,
                    &params.mft_data_runs,
                    mft_stream_offset,
                    &mut buf[..byte_count as usize],
                )
                .map_err(ParallelEnumError::Io)?;
            }

            let records =
                scanner.parse_chunk(&buf[..byte_count as usize], start_record, chunk_count);
            stage_mft_records(
                &mut stmt,
                &records,
                ds_id,
                partition.index,
                &mut path_map,
                &mut deleted_records,
                &mut file_count,
                &mut dir_count,
                &mut total_size,
            )?;

            start_record += chunk_count;
            if let Some(cb) = progress_cb {
                cb(file_count + dir_count, total_size);
            }
        }

        drop(stmt);
        // Drop the read buffer — not needed after chunk processing
        drop(buf);
        backfill_ntfs_directory_index_entries(
            conn,
            ds_id,
            reader,
            partition,
            partition.index,
            &mut path_map,
            &mut file_count,
            &mut dir_count,
        )
        .map_err(|e| {
            ParallelEnumError::MftParams(format!("Backfill NTFS directory index entries: {e}"))
        })?;
        // Capture result before dropping large structures
        let staging_result = update_mft_staging_paths_via_sqlite(
            conn,
            ds_id,
            partition.index,
            &path_map,
            &deleted_records,
        );
        // Free large in-memory structures after path resolution is complete
        drop(path_map); // ~20MB per worker
        drop(deleted_records); // ~3MB per worker
        staging_result
            .map_err(|e| ParallelEnumError::MftParams(format!("Update MFT staging paths: {e}")))?;
        validate_mft_staging_shape(conn, ds_id, partition.index)?;
        Ok((file_count, dir_count, total_size))
    })();

    let stats = match transaction_result {
        Ok(stats) => {
            conn.execute_batch("COMMIT")
                .map_err(ParallelEnumError::Db)?;
            stats
        }
        Err(error) => {
            conn.execute_batch("ROLLBACK").ok();
            return Err(error);
        }
    };

    staging::set_staging_meta(conn, "enum_strategy", "mft")
        .map_err(|e| ParallelEnumError::MftParams(format!("Mark MFT strategy: {e}")))?;
    staging::set_staging_meta(conn, "mft_records", &total_records.to_string())
        .map_err(|e| ParallelEnumError::MftParams(format!("Mark MFT record count: {e}")))?;
    Ok(stats)
}

pub(super) fn validate_mft_staging_shape(
    conn: &rusqlite::Connection,
    ds_id: &str,
    partition_index: usize,
) -> Result<(), ParallelEnumError> {
    let root_id = mft_entry_id(partition_index, 5);
    let suspicious_root_system32 = mft_root_child_count(conn, ds_id, &root_id, "System32")?;
    let suspicious_root_hives = mft_root_child_count(conn, ds_id, &root_id, "SOFTWARE")?
        + mft_root_child_count(conn, ds_id, &root_id, "System.evtx")?;
    let windows_dirs = mft_directory_name_count(conn, ds_id, partition_index, "Windows")?;
    let users_dirs = mft_directory_name_count(conn, ds_id, partition_index, "Users")?;

    if windows_dirs == 0
        && users_dirs == 0
        && (suspicious_root_system32 > 0 || suspicious_root_hives > 0)
    {
        return Err(ParallelEnumError::MftParams(format!(
            "MFT fast path produced suspicious flat NTFS tree: root System32={suspicious_root_system32}, root hive/log candidates={suspicious_root_hives}, Windows dirs={windows_dirs}, Users dirs={users_dirs}. Falling back to recursive NTFS reader."
        )));
    }
    Ok(())
}

fn mft_root_child_count(
    conn: &rusqlite::Connection,
    ds_id: &str,
    root_id: &str,
    name: &str,
) -> Result<i64, String> {
    conn.query_row(
        "SELECT COUNT(*) FROM file_entries
         WHERE data_source_id = ?1 AND parent_id = ?2 AND name = ?3 COLLATE NOCASE",
        params![ds_id, root_id, name],
        |row| row.get(0),
    )
    .map_err(|e| e.to_string())
}

fn mft_directory_name_count(
    conn: &rusqlite::Connection,
    ds_id: &str,
    partition_index: usize,
    name: &str,
) -> Result<i64, String> {
    conn.query_row(
        "SELECT COUNT(*) FROM file_entries
         WHERE data_source_id = ?1
           AND id LIKE ?2
           AND entry_type = 'directory' COLLATE NOCASE
           AND name = ?3 COLLATE NOCASE",
        params![ds_id, format!("mft:{partition_index}:%"), name],
        |row| row.get(0),
    )
    .map_err(|e| e.to_string())
}

#[derive(Debug, Clone)]
pub(super) struct NtfsMftParams {
    pub(super) volume_offset: u64,
    pub(super) mft_cluster: u64,
    pub(super) cluster_size: u64,
    pub(super) record_size: u32,
    pub(super) bytes_per_sector: u16,
    pub(super) mft_data_size: u64,
    pub(super) mft_data_runs: Vec<(i64, u64)>,
}

pub(super) fn read_ntfs_mft_parameters(
    partition: &PartitionWork,
    reader: &mut dyn EvidenceReader,
) -> Result<NtfsMftParams, ParallelEnumError> {
    reader
        .seek(SeekFrom::Start(partition_offset(partition)))
        .map_err(|e| format!("Seek NTFS boot sector: {e}"))?;
    let mut boot = [0u8; 512];
    reader
        .read_exact(&mut boot)
        .map_err(|e| format!("Read NTFS boot sector: {e}"))?;
    if &boot[3..11] != b"NTFS    " {
        return Err(ParallelEnumError::MftParams(
            "not an NTFS boot sector".to_string(),
        ));
    }

    let bytes_per_sector = u16::from_le_bytes([boot[11], boot[12]]);
    let sectors_per_cluster = boot[13];
    if bytes_per_sector == 0 || sectors_per_cluster == 0 {
        return Err(ParallelEnumError::MftParams(
            "invalid NTFS geometry".to_string(),
        ));
    }
    let cluster_size = bytes_per_sector as u64 * sectors_per_cluster as u64;
    let mft_cluster = u64::from_le_bytes(boot[0x30..0x38].try_into().unwrap_or([0; 8]));
    let record_size = mft_record_size_from_boot(&boot);
    let mft_abs_offset = partition_offset(partition) + mft_cluster * cluster_size;
    reader
        .seek(SeekFrom::Start(mft_abs_offset))
        .map_err(|e| format!("Seek MFT record 0: {e}"))?;
    let mut mft_record = vec![0u8; record_size as usize];
    reader
        .read_exact(&mut mft_record)
        .map_err(|e| format!("Read MFT record 0: {e}"))?;
    apply_ntfs_record_fixup(&mut mft_record, bytes_per_sector as usize)
        .map_err(|e| format!("Fix up MFT record 0: {e}"))?;
    let mft_data_size = parse_mft_data_size(&mft_record).unwrap_or(MFT_FALLBACK_SIZE);
    let mft_data_runs = parse_mft_data_runs_from_record(&mft_record)
        .map_err(|e| format!("Parse MFT data runs: {e}"))?;

    Ok(NtfsMftParams {
        volume_offset: partition_offset(partition),
        mft_cluster,
        cluster_size,
        record_size,
        bytes_per_sector,
        mft_data_size,
        mft_data_runs,
    })
}

pub(super) fn open_partition_evidence_reader(
    partition: &PartitionWork,
) -> Result<Box<dyn EvidenceReader>, ParallelEnumError> {
    if partition.source_kind.eq_ignore_ascii_case("e01") {
        Ok(Box::new(
            E01Reader::open(&partition.source_path).map_err(|e| e.to_string())?,
        ))
    } else {
        Ok(Box::new(
            RawImageReader::open(&partition.source_path).map_err(|e| e.to_string())?,
        ))
    }
}

fn partition_offset(partition: &PartitionWork) -> u64 {
    partition.volume_offset
}

fn mft_record_size_from_boot(boot: &[u8]) -> u32 {
    let raw = boot[0x40] as i8;
    if raw > 0 {
        1024
    } else if raw < 0 {
        let shift = (raw as i16).unsigned_abs();
        if shift < 32 {
            (1u32 << shift).max(512)
        } else {
            1024
        }
    } else {
        1024
    }
}

fn parse_mft_data_size(record: &[u8]) -> Option<u64> {
    if record.len() < 4 || &record[0..4] != b"FILE" {
        return None;
    }
    let attr_off = u16::from_le_bytes([record[0x14], record[0x15]]) as usize;
    let mut pos = attr_off;
    while pos + 8 < record.len() {
        let typ = u32::from_le_bytes(record[pos..pos + 4].try_into().ok()?);
        if typ == 0xFFFFFFFF {
            break;
        }
        let len = u32::from_le_bytes(record[pos + 4..pos + 8].try_into().ok()?) as usize;
        if len < 4 || pos + len > record.len() {
            break;
        }
        if typ == 0x80 && pos + 0x38 <= record.len() && (record[pos + 8] & 1) != 0 {
            return Some(u64::from_le_bytes(
                record[pos + 0x30..pos + 0x38].try_into().ok()?,
            ));
        }
        pos += len;
    }
    None
}

fn apply_ntfs_record_fixup(record: &mut [u8], sector_size: usize) -> Result<(), String> {
    if record.len() < 8 || sector_size < 2 {
        return Ok(());
    }

    let usa_offset = u16::from_le_bytes([record[4], record[5]]) as usize;
    let usa_count = u16::from_le_bytes([record[6], record[7]]) as usize;
    if usa_offset == 0 || usa_count < 2 {
        return Ok(());
    }
    let usa_bytes = usa_count
        .checked_mul(2)
        .ok_or_else(|| "invalid update sequence".to_string())?;
    if usa_offset + usa_bytes > record.len() {
        return Err("update sequence array exceeds record length".to_string());
    }

    let expected = [record[usa_offset], record[usa_offset + 1]];
    for index in 1..usa_count {
        let fixup_pos = index
            .checked_mul(sector_size)
            .and_then(|value| value.checked_sub(2))
            .ok_or_else(|| "invalid fixup position".to_string())?;
        if fixup_pos + 2 > record.len() {
            return Err("record too short for update sequence fixup".to_string());
        }
        if record[fixup_pos..fixup_pos + 2] != expected {
            return Err("update sequence signature mismatch".to_string());
        }

        let replacement = usa_offset + index * 2;
        record[fixup_pos] = record[replacement];
        record[fixup_pos + 1] = record[replacement + 1];
    }
    Ok(())
}

fn parse_mft_data_runs_from_record(record: &[u8]) -> Result<Vec<(i64, u64)>, String> {
    if record.len() < 0x18 || &record[0..4] != b"FILE" {
        return Err("MFT record 0 is not a valid FILE record".to_string());
    }

    let attr_off = u16::from_le_bytes([record[0x14], record[0x15]]) as usize;
    let mut pos = attr_off;
    while pos + 8 < record.len() {
        let typ = u32::from_le_bytes(
            record[pos..pos + 4]
                .try_into()
                .map_err(|_| "Invalid MFT attribute type".to_string())?,
        );
        if typ == 0xFFFFFFFF {
            break;
        }
        let len = u32::from_le_bytes(
            record[pos + 4..pos + 8]
                .try_into()
                .map_err(|_| "Invalid MFT attribute length".to_string())?,
        ) as usize;
        if len == 0 || pos + len > record.len() {
            break;
        }

        if typ == 0x80 && pos + 0x40 <= record.len() && (record[pos + 8] & 1) != 0 {
            let run_off = u16::from_le_bytes([record[pos + 0x20], record[pos + 0x21]]) as usize;
            if run_off == 0 || run_off >= len {
                return Ok(Vec::new());
            }
            return parse_ntfs_data_runs(&record[pos + run_off..pos + len]);
        }
        pos += len;
    }
    Ok(Vec::new())
}

fn parse_ntfs_data_runs(mut data: &[u8]) -> Result<Vec<(i64, u64)>, String> {
    const MAX_DATA_RUNS: usize = 100_000;

    let mut runs = Vec::new();
    let mut prev_lcn: i64 = 0;
    while !data.is_empty() && data[0] != 0 {
        if runs.len() >= MAX_DATA_RUNS {
            return Err(format!("too many data runs (limit: {MAX_DATA_RUNS})"));
        }
        let header = data[0];
        let size_bytes = (header & 0x0F) as usize;
        let offset_bytes = ((header >> 4) & 0x0F) as usize;
        if size_bytes > 8 || offset_bytes > 8 {
            break;
        }
        data = &data[1..];
        if data.len() < size_bytes + offset_bytes {
            break;
        }
        let cluster_count = read_sized_le(&data[..size_bytes]);
        data = &data[size_bytes..];
        let lcn_offset = read_sized_le_signed(&data[..offset_bytes]);
        data = &data[offset_bytes..];
        let lcn = if runs.is_empty() {
            lcn_offset
        } else {
            prev_lcn + lcn_offset
        };
        prev_lcn = lcn;
        if cluster_count == 0 {
            continue;
        }
        runs.push((lcn, cluster_count));
    }
    Ok(runs)
}

pub(super) fn read_ntfs_mft_stream(
    reader: &mut dyn EvidenceReader,
    volume_offset: u64,
    cluster_size: u64,
    runs: &[(i64, u64)],
    mut stream_offset: u64,
    out: &mut [u8],
) -> std::io::Result<()> {
    let mut written = 0usize;
    let mut run_stream_start = 0u64;

    for (lcn, cluster_count) in runs {
        if *lcn < 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("negative MFT LCN {lcn}"),
            ));
        }
        let run_bytes = cluster_count.checked_mul(cluster_size).ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("MFT run overflow: {cluster_count} clusters x {cluster_size} bytes"),
            )
        })?;
        let run_end = run_stream_start.saturating_add(run_bytes);
        if stream_offset >= run_end {
            run_stream_start = run_end;
            continue;
        }

        let offset_in_run = stream_offset.saturating_sub(run_stream_start);
        let available = run_bytes.saturating_sub(offset_in_run);
        let to_read = available.min((out.len() - written) as u64) as usize;
        let disk_offset = volume_offset
            .checked_add((*lcn as u64).saturating_mul(cluster_size))
            .and_then(|base| base.checked_add(offset_in_run))
            .ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::InvalidData, "MFT disk offset overflow")
            })?;

        reader.seek(SeekFrom::Start(disk_offset))?;
        reader.read_exact(&mut out[written..written + to_read])?;
        written += to_read;
        if written == out.len() {
            return Ok(());
        }
        stream_offset = run_end;
        run_stream_start = run_end;
    }

    Err(std::io::Error::new(
        std::io::ErrorKind::UnexpectedEof,
        format!(
            "MFT stream ended before read completed (read {} of {} bytes)",
            written,
            out.len()
        ),
    ))
}

fn read_sized_le(bytes: &[u8]) -> u64 {
    let mut value = 0u64;
    for (index, byte) in bytes.iter().enumerate().take(8) {
        value |= (*byte as u64) << (index * 8);
    }
    value
}

fn read_sized_le_signed(bytes: &[u8]) -> i64 {
    let n = bytes.len().min(8);
    if n == 0 {
        return 0;
    }
    let mut value = 0u64;
    for (index, byte) in bytes.iter().enumerate().take(n) {
        value |= (*byte as u64) << (index * 8);
    }
    if bytes[n - 1] & 0x80 != 0 {
        for index in n..8 {
            value |= 0xFFu64 << (index * 8);
        }
    }
    value as i64
}

#[allow(clippy::too_many_arguments)]
fn backfill_ntfs_directory_index_entries(
    conn: &rusqlite::Connection,
    ds_id: &str,
    evidence_reader: Box<dyn EvidenceReader>,
    partition: &PartitionWork,
    partition_index: usize,
    path_map: &mut HashMap<String, (Option<String>, String, bool)>,
    file_count: &mut u64,
    dir_count: &mut u64,
) -> Result<(), String> {
    let ntfs = NtfsReader::open(evidence_reader, partition_offset(partition))
        .map_err(|e| format!("Open NTFS reader for directory indexes: {e}"))?;

    let mut stmt = conn
        .prepare_cached(
            "INSERT OR IGNORE INTO file_entries
             (id, parent_id, data_source_id, path, name, entry_type,
              size, ext, deleted, hidden, system, created_at, modified_at, accessed_at, changed_at, hash_sha256)
             VALUES (?1, ?2, ?3, '', ?4, ?5, ?6, ?7, 0, ?8, ?9, NULL, NULL, NULL, NULL, NULL)",
        )
        .map_err(|e| format!("Prepare NTFS directory index backfill: {e}"))?;

    let mut queue = VecDeque::from([5u64]);
    let mut visited = HashSet::new();
    while let Some(dir_ref) = queue.pop_front() {
        if !visited.insert(dir_ref) {
            continue;
        }
        let entries = match ntfs.list_directory_entries_by_inode(dir_ref) {
            Ok(entries) => entries,
            Err(error) => {
                tracing::warn!("Failed to list NTFS directory index {}: {}", dir_ref, error);
                continue;
            }
        };

        for action in mft_directory_index_backfill_actions(path_map, dir_ref, entries) {
            let entry_id = mft_entry_id(partition_index, action.mft_ref);
            let (hidden, system) =
                visibility_flags_for_name(&action.name, action.hidden, action.system);
            let ext = if action.is_dir {
                None
            } else {
                action
                    .name
                    .rsplit('.')
                    .next()
                    .filter(|ext| *ext != action.name)
                    .map(|ext| ext.to_string())
            };
            let changed = stmt
                .execute(params![
                    entry_id,
                    mft_entry_id(partition_index, dir_ref),
                    ds_id,
                    action.name,
                    if action.is_dir { "directory" } else { "file" },
                    if action.is_dir {
                        None
                    } else {
                        Some(action.size)
                    },
                    ext,
                    hidden as i32,
                    system as i32,
                ])
                .map_err(|e| format!("Insert NTFS directory index backfill row: {e}"))?;
            if changed > 0 {
                if action.is_dir {
                    *dir_count += 1;
                } else {
                    *file_count += 1;
                }
            }

            if action.is_dir && !visited.contains(&action.mft_ref) {
                queue.push_back(action.mft_ref);
            }
        }
    }

    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct MftDirectoryIndexBackfillAction {
    pub(super) name: String,
    pub(super) is_dir: bool,
    pub(super) size: u64,
    pub(super) mft_ref: u64,
    pub(super) hidden: bool,
    pub(super) system: bool,
}

pub(super) fn mft_directory_index_backfill_actions(
    path_map: &mut HashMap<String, (Option<String>, String, bool)>,
    dir_ref: u64,
    entries: Vec<NtfsDirectoryEntry>,
) -> Vec<MftDirectoryIndexBackfillAction> {
    let parent_key = dir_ref.to_string();
    let mut actions = Vec::new();

    for entry in entries {
        if entry.name.is_empty() || entry.mft_ref == dir_ref {
            continue;
        }

        let record_key = entry.mft_ref.to_string();
        if mft_directory_index_entry_should_update(path_map, &record_key, &parent_key, &entry) {
            path_map.insert(
                record_key,
                (Some(parent_key.clone()), entry.name.clone(), entry.is_dir),
            );
        }

        actions.push(MftDirectoryIndexBackfillAction {
            name: entry.name,
            is_dir: entry.is_dir,
            size: entry.size,
            mft_ref: entry.mft_ref,
            hidden: entry.hidden,
            system: entry.system,
        });
    }

    actions
}

fn mft_directory_index_entry_should_update(
    path_map: &HashMap<String, (Option<String>, String, bool)>,
    record_key: &str,
    parent_key: &str,
    entry: &NtfsDirectoryEntry,
) -> bool {
    match path_map.get(record_key) {
        None => true,
        Some((parent, name, is_dir)) => {
            name.is_empty()
                || parent.is_none()
                || parent.as_deref() == Some(record_key)
                || parent
                    .as_deref()
                    .map(|parent| !path_map.contains_key(parent))
                    .unwrap_or(false)
                || (parent.as_deref() == Some("5") && parent_key != "5")
                || parent.as_deref() != Some(parent_key)
                || *is_dir != entry.is_dir
        }
    }
}
#[allow(clippy::too_many_arguments)]
fn stage_mft_records(
    stmt: &mut rusqlite::CachedStatement<'_>,
    records: &[MftRecord],
    ds_id: &str,
    partition_index: usize,
    path_map: &mut HashMap<String, (Option<String>, String, bool)>,
    deleted_records: &mut HashSet<String>,
    file_count: &mut u64,
    dir_count: &mut u64,
    total_size: &mut u64,
) -> Result<(), String> {
    for record in records {
        if !record.is_valid || (record.name.is_empty() && record.record_number != 5) {
            continue;
        }

        let name = if record.record_number == 5 && (record.name.is_empty() || record.name == ".") {
            "\\".to_string()
        } else {
            record.name.clone()
        };
        let parent_key = if record.record_number == 5 {
            None
        } else {
            Some(record.parent_ref.to_string())
        };
        path_map.insert(
            record.record_number.to_string(),
            (parent_key.clone(), name.clone(), record.is_dir),
        );
        if record.deleted {
            deleted_records.insert(record.record_number.to_string());
        }

        let entry_id = mft_entry_id(partition_index, record.record_number);
        let parent_id = parent_key
            .as_deref()
            .map(|parent| mft_entry_id_from_key(partition_index, parent));
        let size = if record.is_dir {
            None
        } else {
            Some(record.size)
        };
        let ext = if record.is_dir {
            None
        } else {
            record
                .name
                .rsplit('.')
                .next()
                .filter(|ext| *ext != record.name)
                .map(|ext| ext.to_string())
        };
        let (hidden, system) = visibility_flags_for_name(&name, record.hidden, record.system);

        stmt.execute(params![
            entry_id,
            parent_id,
            ds_id,
            name,
            if record.is_dir { "directory" } else { "file" },
            size,
            ext,
            record.deleted as i32,
            hidden as i32,
            system as i32,
            record.created_at.as_ref().map(|dt| dt.to_rfc3339()),
            record.modified_at.as_ref().map(|dt| dt.to_rfc3339()),
            record.accessed_at.as_ref().map(|dt| dt.to_rfc3339()),
            record.changed_at.as_ref().map(|dt| dt.to_rfc3339()),
        ])
        .map_err(|e| format!("Insert MFT staging row: {e}"))?;

        if record.is_dir {
            *dir_count += 1;
        } else {
            *file_count += 1;
            *total_size += record.size;
        }
    }

    Ok(())
}

#[cfg(test)]
pub(super) fn records_to_partition_file_entries(
    records: &[MftRecord],
    ds_id: &str,
    partition_index: usize,
) -> Vec<FileEntry> {
    records
        .iter()
        .filter(|record| record.is_valid && (!record.name.is_empty() || record.record_number == 5))
        .map(|record| {
            let name =
                if record.record_number == 5 && (record.name.is_empty() || record.name == ".") {
                    "\\".to_string()
                } else {
                    record.name.clone()
                };
            let entry_type = if record.is_dir {
                EntryType::Directory
            } else {
                EntryType::File
            };
            let ext = if record.is_dir {
                None
            } else {
                record
                    .name
                    .rsplit('.')
                    .next()
                    .filter(|ext| *ext != record.name)
                    .map(|ext| ext.to_string())
            };
            FileEntry {
                id: FileEntryId(mft_entry_id(partition_index, record.record_number)),
                parent_id: if record.record_number == 5 {
                    None
                } else {
                    Some(FileEntryId(mft_entry_id(
                        partition_index,
                        record.parent_ref,
                    )))
                },
                data_source_id: DataSourceId(ds_id.to_string()),
                path: String::new(),
                name,
                entry_type,
                size: if record.is_dir {
                    None
                } else {
                    Some(record.size)
                },
                ext,
                deleted: record.deleted,
                hidden: record.hidden
                    || inferred_hidden_name(&record.name)
                    || inferred_system_name(&record.name),
                system: record.system || inferred_system_name(&record.name),
                encrypted: false,
                created_at: record.created_at,
                modified_at: record.modified_at,
                accessed_at: record.accessed_at,
                changed_at: record.changed_at,
                hash_sha256: None,
            }
        })
        .collect()
}

fn mft_entry_id(partition_index: usize, record_number: u64) -> String {
    format!("mft:{partition_index}:{record_number}")
}

pub(super) fn visibility_flags_for_name(name: &str, hidden: bool, system: bool) -> (bool, bool) {
    let inferred_system = inferred_system_name(name);
    (
        hidden || inferred_hidden_name(name) || inferred_system,
        system || inferred_system,
    )
}

fn inferred_hidden_name(name: &str) -> bool {
    name.starts_with('.')
}

fn inferred_system_name(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "$recycle.bin"
            | "system volume information"
            | "pagefile.sys"
            | "hiberfil.sys"
            | "swapfile.sys"
    )
}

fn mft_entry_id_from_key(partition_index: usize, record_key: &str) -> String {
    format!("mft:{partition_index}:{record_key}")
}

#[cfg(test)]
pub(super) fn mft_record_key(partition_index: usize, entry_id: &str) -> Option<String> {
    entry_id
        .strip_prefix(&format!("mft:{partition_index}:"))
        .map(|value| value.to_string())
}

#[cfg(test)]
pub(super) fn add_partition_entry_to_path_map(
    path_map: &mut HashMap<String, (Option<String>, String, bool)>,
    entry: &FileEntry,
    partition_index: usize,
) {
    let Some(record_num) = mft_record_key(partition_index, &entry.id.0) else {
        return;
    };
    let parent_num = entry
        .parent_id
        .as_ref()
        .and_then(|parent| mft_record_key(partition_index, &parent.0));
    path_map.insert(
        record_num,
        (
            parent_num,
            entry.name.clone(),
            entry.entry_type == EntryType::Directory,
        ),
    );
}

#[cfg(test)]
pub(super) fn update_mft_staging_paths(
    conn: &rusqlite::Connection,
    ds_id: &str,
    partition_index: usize,
    path_map: &HashMap<String, (Option<String>, String, bool)>,
    deleted_records: &HashSet<String>,
) -> rusqlite::Result<()> {
    let mut resolved = HashMap::with_capacity(path_map.len());
    let mut visiting = HashSet::new();
    let records: Vec<String> = path_map.keys().cloned().collect();
    for record in &records {
        resolve_mft_path(
            record,
            path_map,
            deleted_records,
            &mut resolved,
            &mut visiting,
        );
    }

    for (record_num, path) in &resolved {
        FileRepo::update_file_entry_path(
            conn,
            &mft_entry_id_from_key(partition_index, record_num),
            ds_id,
            path,
        )
        .map_err(|e| rusqlite::Error::InvalidParameterName(e.to_string()))?;
    }
    Ok(())
}

#[cfg(test)]
pub(super) fn update_mft_staging_paths_and_parent_ids(
    conn: &rusqlite::Connection,
    ds_id: &str,
    partition_index: usize,
    path_map: &HashMap<String, (Option<String>, String, bool)>,
    deleted_records: &HashSet<String>,
) -> rusqlite::Result<()> {
    let mut resolved = HashMap::with_capacity(path_map.len());
    let mut visiting = HashSet::new();
    for record in path_map.keys() {
        resolve_mft_path(
            record,
            path_map,
            deleted_records,
            &mut resolved,
            &mut visiting,
        );
    }

    let root_id = mft_entry_id_from_key(partition_index, "5");

    for (record_num, (parent, _, _)) in path_map {
        let path = resolved.get(record_num).map(String::as_str).unwrap_or("");
        let entry_id = mft_entry_id_from_key(partition_index, record_num);
        let parent_id = if record_num == "5" {
            None
        } else {
            match parent.as_deref() {
                Some(parent) if parent != record_num && path_map.contains_key(parent) => {
                    Some(mft_entry_id_from_key(partition_index, parent))
                }
                _ if path_map.contains_key("5") => Some(root_id.clone()),
                _ => None,
            }
        };
        FileRepo::update_file_entry_parent_path(conn, &entry_id, ds_id, parent_id.as_deref(), path)
            .map_err(|e| rusqlite::Error::InvalidParameterName(e.to_string()))?;
    }
    Ok(())
}

fn resolve_mft_path(
    record: &str,
    path_map: &HashMap<String, (Option<String>, String, bool)>,
    deleted_records: &HashSet<String>,
    resolved: &mut HashMap<String, String>,
    _visiting: &mut HashSet<String>,
) -> String {
    if let Some(path) = resolved.get(record) {
        return path.clone();
    }

    // Walk up the parent chain iteratively, collecting segments.
    let mut chain: Vec<(String, Option<String>, String)> = Vec::new(); // (id, parent, name)
    let mut current = record.to_string();
    let mut seen: HashSet<String> = HashSet::new();
    seen.insert(current.clone());
    let final_path = loop {
        if let Some(cached) = resolved.get(&current) {
            // Build path from cached root down through the collected chain.
            let mut path = cached.clone();
            for (id, _parent, name) in chain.iter().rev() {
                if path.is_empty() {
                    path = name.clone();
                } else {
                    path.push('/');
                    path.push_str(name);
                }
                resolved.insert(id.clone(), path.clone());
            }
            break resolved.get(record).cloned().unwrap_or_default();
        }
        match path_map.get(&current) {
            Some((Some(parent), name, _))
                if parent != "5"
                    && parent != &current
                    && path_map.contains_key(parent)
                    && seen.insert(parent.clone()) =>
            {
                chain.push((current.clone(), Some(parent.clone()), name.clone()));
                current = parent.clone();
            }
            Some((_, name, _)) => {
                // Reached root, dangling parent, or cycle.
                let base = if current != "5" && deleted_records.contains(&current) {
                    format!("/$DeletedOrphans/{current}-{name}")
                } else {
                    name.clone()
                };
                // Walk the chain in reverse to build full paths and cache them.
                let mut path = base;
                resolved.insert(current.clone(), path.clone());
                for (id, _parent, name) in chain.iter().rev() {
                    path = format!("{path}/{name}");
                    resolved.insert(id.clone(), path.clone());
                }
                break resolved.get(record).cloned().unwrap_or_default();
            }
            None => {
                // Record not in path_map — propagate empty for everything in chain.
                for (id, _, _) in &chain {
                    resolved.insert(id.clone(), String::new());
                }
                resolved.insert(record.to_string(), String::new());
                break String::new();
            }
        }
    };
    final_path
}

#[cfg(test)]
pub(super) fn update_mft_staging_parent_ids(
    conn: &rusqlite::Connection,
    ds_id: &str,
    partition_index: usize,
    path_map: &HashMap<String, (Option<String>, String, bool)>,
) -> rusqlite::Result<()> {
    let mut stmt = conn
        .prepare("UPDATE file_entries SET parent_id = ?1 WHERE id = ?2 AND data_source_id = ?3")?;
    for (record_num, (parent, _, _)) in path_map {
        let entry_id = mft_entry_id_from_key(partition_index, record_num);
        let parent_id = if record_num == "5" {
            None
        } else {
            match parent.as_deref() {
                Some(parent) if parent != record_num && path_map.contains_key(parent) => {
                    Some(mft_entry_id_from_key(partition_index, parent))
                }
                _ if path_map.contains_key("5") => {
                    Some(mft_entry_id_from_key(partition_index, "5"))
                }
                _ => None,
            }
        };
        stmt.execute(params![parent_id, entry_id, ds_id])?;
    }
    Ok(())
}

pub(super) fn update_mft_staging_paths_via_sqlite(
    conn: &rusqlite::Connection,
    ds_id: &str,
    partition_index: usize,
    path_map: &HashMap<String, (Option<String>, String, bool)>,
    deleted_records: &HashSet<String>,
) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TEMP TABLE IF NOT EXISTS mft_path_records (
             record_num TEXT PRIMARY KEY,
             parent_num TEXT,
             name TEXT NOT NULL,
             is_dir INTEGER NOT NULL,
             resolved_path TEXT
         );
         DELETE FROM mft_path_records;",
    )?;
    {
        let mut stmt = conn.prepare_cached(
            "INSERT OR REPLACE INTO mft_path_records
             (record_num, parent_num, name, is_dir)
             VALUES (?1, ?2, ?3, ?4)",
        )?;
        for (record, (parent, name, is_dir)) in path_map {
            stmt.execute(params![record, parent, name, *is_dir as i32])?;
        }
    }

    let mut resolved = HashMap::new();
    let mut visiting = HashSet::new();
    let mut stmt = conn.prepare("SELECT record_num FROM mft_path_records")?;
    let records = stmt
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    for record in &records {
        resolve_mft_path(
            record,
            path_map,
            deleted_records,
            &mut resolved,
            &mut visiting,
        );
    }
    // All paths resolved — records list no longer needed
    drop(records);
    drop(stmt);
    {
        let mut stmt = conn.prepare_cached(
            "UPDATE mft_path_records SET resolved_path = ?1 WHERE record_num = ?2",
        )?;
        for (record, path) in &resolved {
            stmt.execute(params![path, record])?;
        }
    }
    // Resolved paths written to temp table — in-memory map no longer needed (~25MB per worker)
    drop(resolved);
    drop(visiting);
    conn.execute(
        "UPDATE file_entries
         SET path = (
             SELECT resolved_path
             FROM mft_path_records
             WHERE record_num = substr(file_entries.id, ?1)
         )
         WHERE data_source_id = ?2
           AND id LIKE ?3
           AND EXISTS (
             SELECT 1 FROM mft_path_records WHERE record_num = substr(file_entries.id, ?1)
           )",
        params![
            format!("mft:{partition_index}:").len() + 1,
            ds_id,
            format!("mft:{partition_index}:%")
        ],
    )?;
    conn.execute(
        "UPDATE file_entries
         SET parent_id = CASE
             WHEN substr(file_entries.id, ?1) = '5' THEN NULL
             WHEN EXISTS (
                 SELECT 1 FROM mft_path_records parent
                 WHERE parent.record_num = (
                     SELECT child.parent_num
                     FROM mft_path_records child
                     WHERE child.record_num = substr(file_entries.id, ?1)
                 )
             ) THEN ?4 || (
                 SELECT child.parent_num
                 FROM mft_path_records child
                 WHERE child.record_num = substr(file_entries.id, ?1)
             )
             WHEN EXISTS (SELECT 1 FROM mft_path_records WHERE record_num = '5') THEN ?4 || '5'
             ELSE NULL
         END
         WHERE data_source_id = ?2
           AND id LIKE ?3",
        params![
            format!("mft:{partition_index}:").len() + 1,
            ds_id,
            format!("mft:{partition_index}:%"),
            format!("mft:{partition_index}:")
        ],
    )?;
    Ok(())
}
