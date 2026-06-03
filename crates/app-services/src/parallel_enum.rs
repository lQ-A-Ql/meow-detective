//! Parallel filesystem enumeration.
//!
//! Enumerates multiple partitions concurrently, each writing to its own
//! staging DB. After all partitions complete, the caller merges staging
//! DBs into the main case.db.

use crate::staging;
use crossbeam_channel::{bounded, Receiver, Sender};
use domain::{DataSourceId, EntryType, FileEntry, FileEntryId};
use evidence_core::{EvidenceReader, FileSystemReader, RawImageReader};
use fs_ntfs::mft_scanner::{MftRecord, MftScanner};
use image_e01::E01Reader;
use rusqlite::params;
use std::collections::{HashMap, HashSet};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

#[cfg(test)]
const PROGRESS_CHANNEL_CAPACITY: usize = 1;
#[cfg(not(test))]
const PROGRESS_CHANNEL_CAPACITY: usize = 128;

#[cfg(test)]
const ENUM_PROGRESS_INTERVAL: u64 = 1;
#[cfg(not(test))]
const ENUM_PROGRESS_INTERVAL: u64 = 5_000;

const MFT_CHUNK_RECORDS: u64 = 10_000;
const MFT_FALLBACK_SIZE: u64 = 100 * 1024 * 1024;
const MFT_HASHMAP_PATH_LIMIT: usize = 100_000;

/// Info needed to enumerate one partition.
pub struct PartitionWork {
    pub index: usize,
    pub name: String,
    pub fs_kind: String,
    pub fs: Box<dyn FileSystemReader + Send>,
    pub source_path: PathBuf,
    pub source_kind: String,
    pub volume_offset: u64,
}

/// Result from a single partition enumeration.
pub struct PartitionResult {
    pub index: usize,
    pub file_count: u64,
    pub dir_count: u64,
    pub total_size: u64,
    pub warnings: Vec<String>,
    pub error: Option<String>,
}

/// Enumerate multiple partitions in parallel.
///
/// Each thread:
/// 1. Opens its own staging DB
/// 2. Enumerates the filesystem into that staging DB
/// 3. Reports progress via channel
///
/// Returns results for all partitions.
pub fn enumerate_partitions_parallel(
    case_root: &Path,
    data_source_id: &DataSourceId,
    partitions: Vec<PartitionWork>,
    max_workers: usize,
    cancel_token: Arc<AtomicBool>,
    progress_cb: &dyn Fn(usize, u32, &str), // (partition_index, pct, detail)
) -> Result<Vec<PartitionResult>, String> {
    if partitions.is_empty() {
        return Ok(Vec::new());
    }

    let worker_count = partitions.len().min(max_workers.max(1)).max(1);
    let work_capacity = partitions.len().max(1);
    let (work_tx, work_rx): (Sender<PartitionWork>, Receiver<PartitionWork>) =
        bounded(work_capacity);
    let (result_tx, result_rx): (Sender<PartitionResult>, Receiver<PartitionResult>) =
        bounded(worker_count);
    #[allow(clippy::type_complexity)]
    let (progress_tx, progress_rx): (Sender<(usize, u64, u64)>, Receiver<(usize, u64, u64)>) =
        bounded(PROGRESS_CHANNEL_CAPACITY);

    let mut submitted_count = 0usize;
    for partition in partitions {
        if cancel_token.load(Ordering::Relaxed) {
            break;
        }

        progress_cb(partition.index, 0, &format!("Starting {}", partition.name));
        work_tx
            .send(partition)
            .map_err(|e| format!("Failed to queue partition work: {}", e))?;
        submitted_count += 1;
    }
    drop(work_tx);

    if submitted_count == 0 {
        return Ok(Vec::new());
    }

    let active_workers = worker_count.min(submitted_count);
    let mut handles = Vec::with_capacity(active_workers);
    for worker_index in 0..active_workers {
        let tx = result_tx.clone();
        let ptx = progress_tx.clone();
        let rx = work_rx.clone();
        let case_root = case_root.to_path_buf();
        let ds_id = data_source_id.0.clone();
        let cancel = cancel_token.clone();

        let handle = std::thread::Builder::new()
            .name(format!("enum-worker-{}", worker_index))
            .spawn(move || {
                while let Ok(partition) = rx.recv() {
                    let idx = partition.index;
                    if cancel.load(Ordering::Relaxed) {
                        let _ = tx.send(PartitionResult {
                            index: idx,
                            file_count: 0,
                            dir_count: 0,
                            total_size: 0,
                            warnings: Vec::new(),
                            error: Some("Cancelled".to_string()),
                        });
                        break;
                    }

                    let ptx_for_progress = ptx.clone();
                    let progress = move |total_entries: u64, total_size: u64| {
                        let _ = ptx_for_progress.try_send((idx, total_entries, total_size));
                    };
                    let result = enumerate_single_partition(
                        &case_root,
                        &ds_id,
                        partition,
                        &cancel,
                        Some(&progress),
                    );
                    let _ = tx.send(result);

                    if cancel.load(Ordering::Relaxed) {
                        break;
                    }
                }
            })
            .map_err(|e| format!("Failed to spawn thread: {}", e))?;

        handles.push(handle);
    }

    // Drop senders so receivers complete when all threads finish
    drop(result_tx);
    drop(progress_tx);

    let mut results = Vec::with_capacity(submitted_count);
    let mut done_count = 0usize;

    while done_count < submitted_count {
        while let Ok((idx, entries, _total_size)) = progress_rx.try_recv() {
            let pct = heartbeat_percent(done_count, submitted_count, entries);
            progress_cb(idx, pct, &format!("Partition {}: {} entries", idx, entries));
        }

        match result_rx.recv_timeout(Duration::from_millis(25)) {
            Ok(result) => {
                let idx = result.index;
                if result.error.is_some() {
                    progress_cb(idx, 100, &format!("Partition {} failed", idx));
                } else {
                    progress_cb(idx, 100, &format!("Partition {} done", idx));
                }
                results.push(result);
                done_count += 1;
            }
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {}
        }
    }

    while let Ok((idx, entries, _total_size)) = progress_rx.try_recv() {
        let pct = heartbeat_percent(done_count, submitted_count, entries);
        progress_cb(idx, pct, &format!("Partition {}: {} entries", idx, entries));
    }

    let mut worker_panicked = false;
    for handle in handles {
        if let Err(e) = handle.join() {
            tracing::error!("Enumeration thread panicked: {:?}", e);
            worker_panicked = true;
        }
    }
    if worker_panicked {
        return Err("Enumeration worker panicked".to_string());
    }

    Ok(results)
}

fn heartbeat_percent(done_count: usize, submitted_count: usize, entries: u64) -> u32 {
    if submitted_count == 0 {
        return 0;
    }

    let base = ((done_count as u32 * 100) / submitted_count as u32).min(99);
    if entries > 0 {
        base.clamp(3, 99)
    } else {
        base
    }
}

/// Enumerate a single partition into its staging DB.
fn enumerate_single_partition(
    case_root: &Path,
    ds_id: &str,
    partition: PartitionWork,
    cancel_token: &AtomicBool,
    progress_cb: Option<&dyn Fn(u64, u64)>, // (total_entries, total_size)
) -> PartitionResult {
    let idx = partition.index;

    // Open staging DB
    let conn = match staging::open_partition_staging(case_root, ds_id, idx) {
        Ok(conn) => conn,
        Err(e) => {
            return PartitionResult {
                index: idx,
                file_count: 0,
                dir_count: 0,
                total_size: 0,
                warnings: Vec::new(),
                error: Some(format!("Failed to open staging DB: {}", e)),
            };
        }
    };

    // Check if already completed (resume)
    if let Ok(Some(status)) = staging::get_staging_meta(&conn, "status") {
        if status == "done" {
            let file_count = staging::staging_db_row_count(&conn).unwrap_or(0);
            return PartitionResult {
                index: idx,
                file_count,
                dir_count: 0,
                total_size: 0,
                warnings: Vec::new(),
                error: None,
            };
        }
    }

    // Mark as running
    let _ = staging::set_staging_meta(&conn, "status", "running");

    // Enumerate filesystem into staging DB. NTFS gets a best-effort MFT fast
    // path first; any error falls back to recursive reader enumeration.
    let mut warnings = Vec::new();
    let stats = if partition.fs_kind.eq_ignore_ascii_case("ntfs") {
        match enumerate_ntfs_mft_to_staging(&conn, &partition, ds_id, cancel_token, progress_cb) {
            Ok(stats) => Ok(stats),
            Err(error) => {
                tracing::warn!(
                    "MFT fast path failed for partition {}: {}; falling back to recursive enum",
                    idx,
                    error
                );
                let _ = clear_staging_file_entries(&conn);
                let _ = staging::set_staging_meta(&conn, "mft_fallback_warning", &error);
                warnings.push(format!("MFT fast path fallback: {error}"));
                enumerate_fs_to_staging(&conn, &*partition.fs, ds_id, cancel_token, progress_cb)
            }
        }
    } else {
        enumerate_fs_to_staging(&conn, &*partition.fs, ds_id, cancel_token, progress_cb)
    };

    match stats {
        Ok((file_count, dir_count, total_size)) => {
            let _ = staging::set_staging_meta(&conn, "status", "done");
            let _ = staging::set_staging_meta(&conn, "file_count", &file_count.to_string());
            let _ = staging::set_staging_meta(&conn, "dir_count", &dir_count.to_string());
            PartitionResult {
                index: idx,
                file_count,
                dir_count,
                total_size,
                warnings,
                error: None,
            }
        }
        Err(e) => {
            let _ = staging::set_staging_meta(&conn, "status", "failed");
            let _ = staging::set_staging_meta(&conn, "error", &e);
            PartitionResult {
                index: idx,
                file_count: 0,
                dir_count: 0,
                total_size: 0,
                warnings,
                error: Some(e),
            }
        }
    }
}

/// Enumerate a filesystem into a staging DB connection.
///
/// Uses transaction batching: commits every 50K entries to avoid WAL bloat
/// while maintaining high throughput.
fn enumerate_fs_to_staging(
    conn: &rusqlite::Connection,
    fs: &dyn FileSystemReader,
    ds_id: &str,
    cancel_token: &AtomicBool,
    progress_cb: Option<&dyn Fn(u64, u64)>, // (total_entries, total_size)
) -> Result<(u64, u64, u64), String> {
    const COMMIT_INTERVAL: u64 = 50_000;

    if cancel_token.load(Ordering::Relaxed) {
        return Err("Cancelled".to_string());
    }
    let root = fs.root().map_err(|e| e.to_string())?;
    if cancel_token.load(Ordering::Relaxed) {
        return Err("Cancelled".to_string());
    }
    let root_entries = fs.list_children(&root.path).map_err(|e| e.to_string())?;
    if cancel_token.load(Ordering::Relaxed) {
        return Err("Cancelled".to_string());
    }
    let mut file_count = 0u64;
    let mut dir_count = 0u64;
    let mut total_size = 0u64;
    let mut entries_since_commit = 0u64;

    conn.execute_batch("BEGIN TRANSACTION")
        .map_err(|e| format!("Begin transaction: {}", e))?;

    let mut stmt = conn
        .prepare_cached(
            "INSERT INTO file_entries
             (id, parent_id, data_source_id, path, name, entry_type,
              size, ext, deleted, created_at, modified_at, accessed_at, changed_at, hash_sha256)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
        )
        .map_err(|e| format!("Prepare error: {}", e))?;

    let mut stack: Vec<(Vec<evidence_core::FsNode>, Option<String>)> = Vec::new();
    stack.push((root_entries, None));

    while let Some((entries, parent_id)) = stack.pop() {
        if cancel_token.load(Ordering::Relaxed) {
            conn.execute_batch("ROLLBACK").ok();
            return Err("Cancelled".to_string());
        }

        for entry in entries {
            if cancel_token.load(Ordering::Relaxed) {
                conn.execute_batch("ROLLBACK").ok();
                return Err("Cancelled".to_string());
            }

            let entry_id = uuid::Uuid::new_v4().to_string();
            let is_dir = entry.is_dir;
            let size = entry.size;

            stmt.execute(rusqlite::params![
                entry_id,
                parent_id,
                ds_id,
                entry.path,
                entry.name,
                if is_dir { "Directory" } else { "File" },
                Some(size),
                entry.name.rsplit('.').next().map(|e| e.to_lowercase()),
                0i32,
                entry.created_at.as_ref().map(|dt| dt.to_rfc3339()),
                entry.modified_at.as_ref().map(|dt| dt.to_rfc3339()),
                entry.accessed_at.as_ref().map(|dt| dt.to_rfc3339()),
                None::<String>,
                None::<String>,
            ])
            .map_err(|e| format!("Insert error: {}", e))?;

            entries_since_commit += 1;
            if is_dir {
                dir_count += 1;
            } else {
                file_count += 1;
                total_size += size;
            }

            // Commit and restart transaction every COMMIT_INTERVAL entries
            if entries_since_commit >= COMMIT_INTERVAL {
                drop(stmt);
                conn.execute_batch("COMMIT")
                    .map_err(|e| format!("Commit error: {}", e))?;
                conn.execute_batch("BEGIN TRANSACTION")
                    .map_err(|e| format!("Begin transaction error: {}", e))?;
                stmt = conn
                    .prepare_cached(
                        "INSERT INTO file_entries
                         (id, parent_id, data_source_id, path, name, entry_type,
                          size, ext, deleted, created_at, modified_at, accessed_at, changed_at, hash_sha256)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
                    )
                    .map_err(|e| format!("Prepare error: {}", e))?;
                entries_since_commit = 0;
            }

            // Report progress every 5000 entries
            let total_entries = file_count + dir_count;
            if total_entries.is_multiple_of(ENUM_PROGRESS_INTERVAL) {
                if let Some(cb) = progress_cb {
                    cb(total_entries, total_size);
                }
            }

            if is_dir {
                if cancel_token.load(Ordering::Relaxed) {
                    conn.execute_batch("ROLLBACK").ok();
                    return Err("Cancelled".to_string());
                }
                match fs.list_children(&entry.path) {
                    Ok(children) => {
                        if cancel_token.load(Ordering::Relaxed) {
                            conn.execute_batch("ROLLBACK").ok();
                            return Err("Cancelled".to_string());
                        }
                        stack.push((children, Some(entry_id)));
                    }
                    Err(e) => {
                        tracing::warn!("Failed to list {}: {}", entry.path, e);
                    }
                }
            }
        }
    }

    drop(stmt);
    conn.execute_batch("COMMIT")
        .map_err(|e| format!("Commit error: {}", e))?;

    Ok((file_count, dir_count, total_size))
}

fn enumerate_ntfs_mft_to_staging(
    conn: &rusqlite::Connection,
    partition: &PartitionWork,
    ds_id: &str,
    cancel_token: &AtomicBool,
    progress_cb: Option<&dyn Fn(u64, u64)>,
) -> Result<(u64, u64, u64), String> {
    if cancel_token.load(Ordering::Relaxed) {
        return Err("Cancelled".to_string());
    }

    let params = read_ntfs_mft_parameters(partition)?;
    if params.mft_data_size == 0 {
        return Err("MFT data size is zero".to_string());
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
        return Err("MFT total record count is zero".to_string());
    }

    conn.execute_batch("BEGIN TRANSACTION")
        .map_err(|e| format!("Begin MFT staging transaction: {e}"))?;
    let transaction_result = (|| {
        let mut stmt = conn
            .prepare_cached(
                "INSERT OR IGNORE INTO file_entries
                 (id, parent_id, data_source_id, path, name, entry_type,
                  size, ext, deleted, created_at, modified_at, accessed_at, changed_at, hash_sha256)
                 VALUES (?1, ?2, ?3, '', ?4, ?5, ?6, ?7, 0, ?8, ?9, ?10, ?11, NULL)",
            )
            .map_err(|e| format!("Prepare MFT staging insert: {e}"))?;

        let mut reader = open_partition_evidence_reader(partition)?;
        let mut start_record = 0u64;
        let mut file_count = 0u64;
        let mut dir_count = 0u64;
        let mut total_size = 0u64;
        let mut path_map: HashMap<String, (Option<String>, String, bool)> = HashMap::new();

        while start_record < total_records {
            if cancel_token.load(Ordering::Relaxed) {
                return Err("Cancelled".to_string());
            }

            let chunk_count = MFT_CHUNK_RECORDS.min(total_records - start_record);
            let byte_offset =
                scanner.mft_abs_offset() + start_record * scanner.record_size() as u64;
            let byte_count = chunk_count * scanner.record_size() as u64;
            reader
                .seek(SeekFrom::Start(byte_offset))
                .map_err(|e| format!("Seek MFT chunk {start_record}: {e}"))?;
            let mut buf = vec![0u8; byte_count as usize];
            reader
                .read_exact(&mut buf)
                .map_err(|e| format!("Read MFT chunk {start_record}: {e}"))?;

            let records = scanner.parse_chunk(&buf, start_record, chunk_count);
            for entry in records_to_partition_file_entries(&records, ds_id, partition.index) {
                add_partition_entry_to_path_map(&mut path_map, &entry, partition.index);
                match entry.entry_type {
                    EntryType::Directory => dir_count += 1,
                    EntryType::File => {
                        file_count += 1;
                        total_size += entry.size.unwrap_or(0);
                    }
                }
                stmt.execute(params![
                    entry.id.0,
                    entry.parent_id.as_ref().map(|id| &id.0),
                    entry.data_source_id.0,
                    entry.name,
                    if entry.entry_type == EntryType::Directory {
                        "Directory"
                    } else {
                        "File"
                    },
                    entry.size,
                    entry.ext,
                    entry.created_at.as_ref().map(|dt| dt.to_rfc3339()),
                    entry.modified_at.as_ref().map(|dt| dt.to_rfc3339()),
                    entry.accessed_at.as_ref().map(|dt| dt.to_rfc3339()),
                    entry.changed_at.as_ref().map(|dt| dt.to_rfc3339()),
                ])
                .map_err(|e| format!("Insert MFT staging row: {e}"))?;
            }

            start_record += chunk_count;
            if let Some(cb) = progress_cb {
                cb(file_count + dir_count, total_size);
            }
        }

        drop(stmt);
        if path_map.len() > MFT_HASHMAP_PATH_LIMIT {
            update_mft_staging_paths_via_sqlite(conn, ds_id, partition.index, &path_map)
                .map_err(|e| format!("Update large MFT staging paths: {e}"))?;
        } else {
            update_mft_staging_paths(conn, ds_id, partition.index, &path_map)
                .map_err(|e| format!("Update MFT staging paths: {e}"))?;
            update_mft_staging_parent_ids(conn, ds_id, partition.index, &path_map)
                .map_err(|e| format!("Update MFT staging parents: {e}"))?;
        }
        Ok((file_count, dir_count, total_size))
    })();

    let stats = match transaction_result {
        Ok(stats) => {
            conn.execute_batch("COMMIT")
                .map_err(|e| format!("Commit MFT staging transaction: {e}"))?;
            stats
        }
        Err(error) => {
            conn.execute_batch("ROLLBACK").ok();
            return Err(error);
        }
    };

    staging::set_staging_meta(conn, "enum_strategy", "mft")
        .map_err(|e| format!("Mark MFT strategy: {e}"))?;
    staging::set_staging_meta(conn, "mft_records", &total_records.to_string())
        .map_err(|e| format!("Mark MFT record count: {e}"))?;
    Ok(stats)
}

#[derive(Debug, Clone, Copy)]
struct NtfsMftParams {
    volume_offset: u64,
    mft_cluster: u64,
    cluster_size: u64,
    record_size: u32,
    bytes_per_sector: u16,
    mft_data_size: u64,
}

fn read_ntfs_mft_parameters(partition: &PartitionWork) -> Result<NtfsMftParams, String> {
    let mut reader = open_partition_evidence_reader(partition)?;
    reader
        .seek(SeekFrom::Start(partition_offset(partition)))
        .map_err(|e| format!("Seek NTFS boot sector: {e}"))?;
    let mut boot = [0u8; 512];
    reader
        .read_exact(&mut boot)
        .map_err(|e| format!("Read NTFS boot sector: {e}"))?;
    if &boot[3..11] != b"NTFS    " {
        return Err("not an NTFS boot sector".to_string());
    }

    let bytes_per_sector = u16::from_le_bytes([boot[11], boot[12]]);
    let sectors_per_cluster = boot[13];
    if bytes_per_sector == 0 || sectors_per_cluster == 0 {
        return Err("invalid NTFS geometry".to_string());
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
    let mft_data_size = parse_mft_data_size(&mft_record).unwrap_or(MFT_FALLBACK_SIZE);

    Ok(NtfsMftParams {
        volume_offset: partition_offset(partition),
        mft_cluster,
        cluster_size,
        record_size,
        bytes_per_sector,
        mft_data_size,
    })
}

fn open_partition_evidence_reader(
    partition: &PartitionWork,
) -> Result<Box<dyn EvidenceReader>, String> {
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

fn records_to_partition_file_entries(
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
                deleted: false,
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

fn mft_entry_id_from_key(partition_index: usize, record_key: &str) -> String {
    format!("mft:{partition_index}:{record_key}")
}

fn mft_record_key(partition_index: usize, entry_id: &str) -> Option<String> {
    entry_id
        .strip_prefix(&format!("mft:{partition_index}:"))
        .map(|value| value.to_string())
}

fn add_partition_entry_to_path_map(
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

fn update_mft_staging_paths(
    conn: &rusqlite::Connection,
    ds_id: &str,
    partition_index: usize,
    path_map: &HashMap<String, (Option<String>, String, bool)>,
) -> rusqlite::Result<()> {
    let mut resolved = HashMap::with_capacity(path_map.len());
    let mut visiting = HashSet::new();
    let records: Vec<String> = path_map.keys().cloned().collect();
    for record in &records {
        resolve_mft_path(record, path_map, &mut resolved, &mut visiting);
    }

    let mut stmt =
        conn.prepare("UPDATE file_entries SET path = ?1 WHERE id = ?2 AND data_source_id = ?3")?;
    for (record_num, path) in &resolved {
        stmt.execute(params![
            path,
            mft_entry_id_from_key(partition_index, record_num),
            ds_id
        ])?;
    }
    Ok(())
}

fn resolve_mft_path(
    record: &str,
    path_map: &HashMap<String, (Option<String>, String, bool)>,
    resolved: &mut HashMap<String, String>,
    visiting: &mut HashSet<String>,
) -> String {
    if let Some(path) = resolved.get(record) {
        return path.clone();
    }
    if !visiting.insert(record.to_string()) {
        return String::new();
    }
    let (parent, name, _) = match path_map.get(record) {
        Some(value) => value,
        None => {
            visiting.remove(record);
            return String::new();
        }
    };
    let path = match parent {
        Some(parent) if parent != "5" && path_map.contains_key(parent) => {
            let parent_path = resolve_mft_path(parent, path_map, resolved, visiting);
            if parent_path.is_empty() {
                name.clone()
            } else {
                format!("{parent_path}/{name}")
            }
        }
        _ => name.clone(),
    };
    resolved.insert(record.to_string(), path.clone());
    visiting.remove(record);
    path
}

fn update_mft_staging_parent_ids(
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

fn update_mft_staging_paths_via_sqlite(
    conn: &rusqlite::Connection,
    ds_id: &str,
    partition_index: usize,
    path_map: &HashMap<String, (Option<String>, String, bool)>,
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
        resolve_mft_path(record, path_map, &mut resolved, &mut visiting);
    }
    {
        let mut stmt = conn.prepare_cached(
            "UPDATE mft_path_records SET resolved_path = ?1 WHERE record_num = ?2",
        )?;
        for (record, path) in &resolved {
            stmt.execute(params![path, record])?;
        }
    }
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

fn clear_staging_file_entries(conn: &rusqlite::Connection) -> rusqlite::Result<()> {
    conn.execute_batch("DELETE FROM file_entries")
}

/// Get the default number of workers (all logical cores).
pub fn default_worker_count() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
}

/// Resolve worker count from settings or default.
pub fn resolve_worker_count(max_import_workers: Option<usize>) -> usize {
    match max_import_workers {
        Some(n) if n > 0 => n.min(default_worker_count()),
        _ => default_worker_count(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use evidence_core::filesystem::root_node;
    use evidence_core::FsNode;
    use std::io::{self, Cursor, Read};
    use std::sync::atomic::AtomicUsize;
    use std::sync::Mutex;
    use std::time::{Duration, Instant};

    struct FakeFsReader {
        name: String,
        entry_count: usize,
        root_list_delay: Duration,
        active_lists: Arc<AtomicUsize>,
        max_active_lists: Arc<AtomicUsize>,
    }

    struct ActiveListGuard<'a> {
        active_lists: &'a AtomicUsize,
    }

    impl Drop for ActiveListGuard<'_> {
        fn drop(&mut self) {
            self.active_lists.fetch_sub(1, Ordering::SeqCst);
        }
    }

    impl FakeFsReader {
        fn new(
            name: impl Into<String>,
            entry_count: usize,
            root_list_delay: Duration,
            active_lists: Arc<AtomicUsize>,
            max_active_lists: Arc<AtomicUsize>,
        ) -> Self {
            Self {
                name: name.into(),
                entry_count,
                root_list_delay,
                active_lists,
                max_active_lists,
            }
        }

        fn root_files(&self) -> Vec<FsNode> {
            (0..self.entry_count)
                .map(|index| FsNode {
                    name: format!("file-{index}.txt"),
                    path: format!("/file-{index}.txt"),
                    is_dir: false,
                    size: 1,
                    created_at: None,
                    modified_at: None,
                    accessed_at: None,
                })
                .collect()
        }
    }

    impl FileSystemReader for FakeFsReader {
        fn root(&self) -> io::Result<FsNode> {
            Ok(root_node())
        }

        fn list_children(&self, path: &str) -> io::Result<Vec<FsNode>> {
            if !path.is_empty() {
                return Ok(Vec::new());
            }

            let active = self.active_lists.fetch_add(1, Ordering::SeqCst) + 1;
            update_max_active(&self.max_active_lists, active);
            let _guard = ActiveListGuard {
                active_lists: &self.active_lists,
            };

            if !self.root_list_delay.is_zero() {
                std::thread::sleep(self.root_list_delay);
            }

            Ok(self.root_files())
        }

        fn open_file(&self, _path: &str) -> io::Result<Box<dyn Read>> {
            Ok(Box::new(Cursor::new(Vec::<u8>::new())))
        }

        fn data_source_name(&self) -> &str {
            &self.name
        }
    }

    fn update_max_active(max_active_lists: &AtomicUsize, active: usize) {
        let mut current = max_active_lists.load(Ordering::SeqCst);
        while active > current {
            match max_active_lists.compare_exchange(
                current,
                active,
                Ordering::SeqCst,
                Ordering::SeqCst,
            ) {
                Ok(_) => break,
                Err(next) => current = next,
            }
        }
    }

    fn fake_partition_work(
        index: usize,
        entry_count: usize,
        root_list_delay: Duration,
        active_lists: Arc<AtomicUsize>,
        max_active_lists: Arc<AtomicUsize>,
    ) -> PartitionWork {
        PartitionWork {
            index,
            name: format!("Partition {index}"),
            fs_kind: "FakeFs".to_string(),
            fs: Box::new(FakeFsReader::new(
                format!("fake-{index}"),
                entry_count,
                root_list_delay,
                active_lists,
                max_active_lists,
            )),
            source_path: PathBuf::from(format!("fake-{index}.img")),
            source_kind: "Raw".to_string(),
            volume_offset: 0,
        }
    }

    fn fake_partitions(
        count: usize,
        entry_count: usize,
        root_list_delay: Duration,
    ) -> Vec<PartitionWork> {
        let active_lists = Arc::new(AtomicUsize::new(0));
        let max_active_lists = Arc::new(AtomicUsize::new(0));
        (0..count)
            .map(|index| {
                fake_partition_work(
                    index,
                    entry_count,
                    root_list_delay,
                    active_lists.clone(),
                    max_active_lists.clone(),
                )
            })
            .collect()
    }

    fn fake_mft_record(record_number: u64, parent_ref: u64, name: &str, is_dir: bool) -> MftRecord {
        MftRecord {
            record_number,
            name: name.to_string(),
            parent_ref,
            is_dir,
            size: if is_dir { 0 } else { 12 },
            created_at: None,
            modified_at: None,
            accessed_at: None,
            changed_at: None,
            is_valid: true,
        }
    }

    #[test]
    fn test_default_worker_count() {
        let count = default_worker_count();
        assert!(count >= 1);
    }

    #[test]
    fn test_resolve_worker_count() {
        assert_eq!(resolve_worker_count(None), default_worker_count());
        assert_eq!(resolve_worker_count(Some(0)), default_worker_count());
        assert_eq!(resolve_worker_count(Some(2)), 2.min(default_worker_count()));
        assert_eq!(resolve_worker_count(Some(999)), default_worker_count());
    }

    #[test]
    fn test_resolve_worker_count_one() {
        assert_eq!(resolve_worker_count(Some(1)), 1);
    }

    #[test]
    fn ntfs_mft_fast_path_writes_partition_prefixed_ids() {
        let records = vec![
            fake_mft_record(5, 5, ".", true),
            fake_mft_record(42, 5, "Windows", true),
            fake_mft_record(43, 42, "notepad.exe", false),
        ];

        let entries = records_to_partition_file_entries(&records, "ds-1", 3);
        let root = entries
            .iter()
            .find(|entry| entry.id.0 == "mft:3:5")
            .unwrap();
        assert!(root.parent_id.is_none());
        let child = entries
            .iter()
            .find(|entry| entry.id.0 == "mft:3:43")
            .unwrap();
        assert_eq!(
            child.parent_id.as_ref().map(|id| id.0.as_str()),
            Some("mft:3:42")
        );
    }

    fn insert_staging_entry(conn: &rusqlite::Connection, id: &str, ds_id: &str) {
        conn.execute(
            "INSERT INTO file_entries (id, data_source_id, path, name, entry_type)
             VALUES (?1, ?2, '', ?3, 'File')",
            rusqlite::params![id, ds_id, id],
        )
        .unwrap();
    }

    #[test]
    fn ntfs_mft_updates_paths_and_parent_ids() {
        let conn = persistence_sqlite::connection::open_in_memory().unwrap();
        conn.execute_batch(include_str!(
            "../../persistence-sqlite/src/migrations/scripts/staging_001.sql"
        ))
        .unwrap();

        let records = vec![
            fake_mft_record(5, 5, ".", true),
            fake_mft_record(42, 5, "Windows", true),
            fake_mft_record(43, 42, "notepad.exe", false),
        ];
        let entries = records_to_partition_file_entries(&records, "ds-mft", 3);
        let mut path_map = HashMap::new();
        for entry in &entries {
            insert_staging_entry(&conn, &entry.id.0, "ds-mft");
            add_partition_entry_to_path_map(&mut path_map, entry, 3);
        }

        update_mft_staging_paths(&conn, "ds-mft", 3, &path_map).unwrap();
        update_mft_staging_parent_ids(&conn, "ds-mft", 3, &path_map).unwrap();

        let (path, parent_id): (String, Option<String>) = conn
            .query_row(
                "SELECT path, parent_id FROM file_entries WHERE id = 'mft:3:43'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(path, "Windows/notepad.exe");
        assert_eq!(parent_id.as_deref(), Some("mft:3:42"));
    }

    #[test]
    fn mft_large_record_count_uses_sqlite_resolver() {
        let conn = persistence_sqlite::connection::open_in_memory().unwrap();
        conn.execute_batch(include_str!(
            "../../persistence-sqlite/src/migrations/scripts/staging_001.sql"
        ))
        .unwrap();
        let ds_id = "ds-mft-large";
        insert_staging_entry(&conn, "mft:9:5", ds_id);
        insert_staging_entry(&conn, "mft:9:42", ds_id);
        insert_staging_entry(&conn, "mft:9:43", ds_id);
        let mut path_map = HashMap::new();
        path_map.insert("5".to_string(), (None, "\\".to_string(), true));
        path_map.insert(
            "42".to_string(),
            (Some("5".to_string()), "Windows".to_string(), true),
        );
        path_map.insert(
            "43".to_string(),
            (Some("42".to_string()), "notepad.exe".to_string(), false),
        );

        update_mft_staging_paths_via_sqlite(&conn, ds_id, 9, &path_map).unwrap();

        let (path, parent_id): (String, Option<String>) = conn
            .query_row(
                "SELECT path, parent_id FROM file_entries WHERE id = 'mft:9:43'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(path, "Windows/notepad.exe");
        assert_eq!(parent_id.as_deref(), Some("mft:9:42"));
    }

    #[test]
    fn ntfs_mft_update_uses_record_key_without_numeric_fallback() {
        let conn = persistence_sqlite::connection::open_in_memory().unwrap();
        conn.execute_batch(include_str!(
            "../../persistence-sqlite/src/migrations/scripts/staging_001.sql"
        ))
        .unwrap();
        insert_staging_entry(&conn, "mft:8:0", "ds-mft");
        insert_staging_entry(&conn, "mft:8:bad-key", "ds-mft");

        let mut path_map = HashMap::new();
        path_map.insert("5".to_string(), (None, "\\".to_string(), true));
        path_map.insert(
            "bad-key".to_string(),
            (Some("5".to_string()), "orphan.bin".to_string(), false),
        );

        update_mft_staging_paths(&conn, "ds-mft", 8, &path_map).unwrap();
        update_mft_staging_parent_ids(&conn, "ds-mft", 8, &path_map).unwrap();

        let bad: (String, Option<String>) = conn
            .query_row(
                "SELECT path, parent_id FROM file_entries WHERE id = 'mft:8:bad-key'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        let zero_path: String = conn
            .query_row(
                "SELECT path FROM file_entries WHERE id = 'mft:8:0'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(bad.0, "orphan.bin");
        assert_eq!(bad.1.as_deref(), Some("mft:8:5"));
        assert_eq!(zero_path, "");
    }

    #[test]
    fn ntfs_mft_fast_path_fallback_records_warning() {
        let tmp = tempfile::TempDir::new().unwrap();
        let active_lists = Arc::new(AtomicUsize::new(0));
        let max_active_lists = Arc::new(AtomicUsize::new(0));
        let mut partition =
            fake_partition_work(0, 3, Duration::ZERO, active_lists, max_active_lists);
        partition.fs_kind = "ntfs".to_string();
        partition.source_path = tmp.path().join("missing.raw");

        let result = enumerate_single_partition(
            tmp.path(),
            "ds-mft-fallback",
            partition,
            &AtomicBool::new(false),
            None,
        );

        assert!(result.error.is_none());
        assert_eq!(result.file_count, 3);
        assert_eq!(result.warnings.len(), 1);
        assert!(result.warnings[0].starts_with("MFT fast path fallback:"));
        let conn = staging::open_partition_staging(tmp.path(), "ds-mft-fallback", 0).unwrap();
        let warning = staging::get_staging_meta(&conn, "mft_fallback_warning")
            .unwrap()
            .unwrap();
        assert!(!warning.trim().is_empty());
        conn.execute(
            "INSERT INTO staging_meta (key, value) VALUES ('post_fallback_write', 'ok')",
            [],
        )
        .unwrap();
    }

    #[test]
    fn parallel_enum_respects_max_workers() {
        let tmp = tempfile::TempDir::new().unwrap();
        let active_lists = Arc::new(AtomicUsize::new(0));
        let max_active_lists = Arc::new(AtomicUsize::new(0));
        let partitions = (0..4)
            .map(|index| {
                fake_partition_work(
                    index,
                    1,
                    Duration::from_millis(25),
                    active_lists.clone(),
                    max_active_lists.clone(),
                )
            })
            .collect();

        let results = enumerate_partitions_parallel(
            tmp.path(),
            &DataSourceId("ds-max-workers".to_string()),
            partitions,
            1,
            Arc::new(AtomicBool::new(false)),
            &|_, _, _| {},
        )
        .unwrap();

        assert_eq!(results.len(), 4);
        assert!(results.iter().all(|result| result.error.is_none()));
        assert_eq!(max_active_lists.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn parallel_enum_uses_external_cancel_token() {
        let tmp = tempfile::TempDir::new().unwrap();
        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_for_thread = cancel.clone();
        let active_lists = Arc::new(AtomicUsize::new(0));
        let max_active_lists = Arc::new(AtomicUsize::new(0));
        let partitions = vec![fake_partition_work(
            0,
            10,
            Duration::from_millis(75),
            active_lists,
            max_active_lists,
        )];

        let canceller = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(10));
            cancel_for_thread.store(true, Ordering::Relaxed);
        });

        let results = enumerate_partitions_parallel(
            tmp.path(),
            &DataSourceId("ds-cancel".to_string()),
            partitions,
            1,
            cancel,
            &|_, _, _| {},
        )
        .unwrap();
        canceller.join().unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].file_count, 0);
        assert_eq!(results[0].error.as_deref(), Some("Cancelled"));
    }

    #[test]
    fn progress_backpressure_does_not_block_worker() {
        let tmp = tempfile::TempDir::new().unwrap();
        let started = Instant::now();
        let progress_events = Arc::new(AtomicUsize::new(0));
        let progress_events_for_cb = progress_events.clone();

        let results = enumerate_partitions_parallel(
            tmp.path(),
            &DataSourceId("ds-progress-backpressure".to_string()),
            fake_partitions(1, 10_000, Duration::ZERO),
            1,
            Arc::new(AtomicBool::new(false)),
            &|_, _, _| {
                progress_events_for_cb.fetch_add(1, Ordering::SeqCst);
                std::thread::sleep(Duration::from_millis(2));
            },
        )
        .unwrap();

        assert_eq!(results.len(), 1);
        assert!(results[0].error.is_none());
        assert!(progress_events.load(Ordering::SeqCst) > 0);
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "progress backpressure blocked enumeration for {:?}",
            started.elapsed()
        );
    }

    #[test]
    fn single_partition_emits_entry_progress() {
        let tmp = tempfile::TempDir::new().unwrap();
        let progress = Arc::new(Mutex::new(Vec::<(u32, String)>::new()));
        let progress_for_cb = progress.clone();

        let results = enumerate_partitions_parallel(
            tmp.path(),
            &DataSourceId("ds-single-heartbeat".to_string()),
            fake_partitions(1, 10_000, Duration::ZERO),
            4,
            Arc::new(AtomicBool::new(false)),
            &|_, pct, detail| {
                progress_for_cb
                    .lock()
                    .unwrap()
                    .push((pct, detail.to_string()));
            },
        )
        .unwrap();

        assert_eq!(results.len(), 1);
        assert!(results[0].error.is_none());
        let progress = progress.lock().unwrap();
        assert!(progress.iter().any(|(pct, detail)| {
            *pct > 0
                && *pct < 100
                && detail.starts_with("Partition 0:")
                && detail.ends_with("entries")
        }));
    }

    #[test]
    fn test_staging_db_insert_and_count() {
        let conn = persistence_sqlite::connection::open_in_memory().unwrap();
        conn.execute_batch(include_str!(
            "../../persistence-sqlite/src/migrations/scripts/staging_001.sql"
        ))
        .unwrap();

        conn.execute_batch("BEGIN TRANSACTION").unwrap();
        for i in 0..5 {
            conn.execute(
                "INSERT INTO file_entries (id, data_source_id, path, name, entry_type) VALUES (?1, ?2, ?3, ?4, 'File')",
                rusqlite::params![
                    format!("f{}", i),
                    "ds-1",
                    format!("/test/file{}.txt", i),
                    format!("file{}.txt", i),
                ],
            )
            .unwrap();
        }
        conn.execute_batch("COMMIT").unwrap();

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM file_entries", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 5);
    }

    #[test]
    fn test_staging_db_preserves_data() {
        let conn = persistence_sqlite::connection::open_in_memory().unwrap();
        conn.execute_batch(include_str!(
            "../../persistence-sqlite/src/migrations/scripts/staging_001.sql"
        ))
        .unwrap();

        conn.execute(
            "INSERT INTO file_entries (id, parent_id, data_source_id, path, name, entry_type, size, ext)
             VALUES (?1, ?2, ?3, ?4, ?5, 'File', 4096, 'pdf')",
            rusqlite::params!["data-test", "parent-1", "ds-x", "/root/doc.pdf", "doc.pdf"],
        )
        .unwrap();

        let (path, name, entry_type): (String, String, String) = conn
            .query_row(
                "SELECT path, name, entry_type FROM file_entries WHERE id = 'data-test'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(path, "/root/doc.pdf");
        assert_eq!(name, "doc.pdf");
        assert_eq!(entry_type, "File");
    }
}
