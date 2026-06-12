use domain::{DataSourceId, EntryType, FileEntry, FileEntryId};
use evidence_core::EvidenceReader;
use image_e01::E01Reader;
use persistence_sqlite::{repositories::file_repo::FileRepo, DbError, DbResult};
use rusqlite::Connection;
use std::{
    collections::{HashMap, HashSet},
    io::SeekFrom,
    path::Path,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
    thread,
};

mod data_sources;
mod enumeration;
mod file_rows;
mod mapping;
mod partition_roots;
mod sort;
mod tree_queries;
mod viewer;
mod visibility;

pub use data_sources::{get_data_sources_real, get_recent_objects_real, rename_data_source_real};
pub use enumeration::{
    enumerate_filesystem, enumerate_filesystem_with_root_name,
    enumerate_filesystem_with_root_name_and_cancel, EnumerationStats,
};
pub use file_rows::get_file_rows_for_request;
pub use partition_roots::{
    insert_partition_placeholder_root, replace_placeholder_root_with_real,
    store_data_source_partitions,
};
pub use tree_queries::{
    get_file_children_lazy, get_file_children_lazy_with_visibility, get_file_tree_real,
    get_file_tree_real_with_visibility,
};
pub use viewer::{
    get_file_path_for_entry, open_file_content_by_id, open_file_handle_real,
    read_file_header_by_id, read_file_range_for_case, read_file_range_real, safe_relative_path,
    skip_reader_bytes,
};

// ============================================================================
// MFT-based bulk NTFS enumeration with multi-threading
// ============================================================================

use crossbeam_channel::{bounded, Receiver, Sender};
use fs_ntfs::mft_scanner::{MftRecord, MftScanner};

const MFT_CHUNK_RECORDS: u64 = 10_000;
const MFT_CHANNEL_BOUND: usize = 4;
const MFT_DB_BATCH_SIZE: usize = 2_000;

/// Multi-threaded MFT-based NTFS enumeration.
///
/// Architecture:
///   Reader Thread → channel → Parser Thread Pool → channel → DB Writer Thread
///
/// - Reader: Sequentially reads MFT chunks from E01
/// - Parsers: Parse FILE records in parallel (CPU-bound)
/// - Writer: Batch-inserts FileEntry into SQLite
///
/// After all records are processed, reconstructs full paths via parent_ref chains.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn enumerate_filesystem_mft(
    conn: &Connection,
    data_source_id: &DataSourceId,
    e01_path: &Path,
    volume_offset: u64,
    mft_cluster: u64,
    cluster_size: u64,
    record_size: u32,
    bytes_per_sector: u16,
    mft_data_size: u64,
    progress_fn: Option<&dyn Fn(u32, &str)>,
    cancel: Option<Arc<AtomicBool>>,
) -> DbResult<EnumerationStats> {
    let scanner = MftScanner::new(
        volume_offset,
        mft_cluster,
        cluster_size,
        record_size,
        bytes_per_sector,
        mft_data_size,
    );
    let total_records = scanner.total_records();
    let scanner_record_size = scanner.record_size();

    if let Some(pf) = progress_fn {
        pf(5, "Starting MFT scan...");
    }

    let mft_data_runs = read_ntfs_mft_data_runs(
        e01_path,
        volume_offset,
        mft_cluster,
        cluster_size,
        record_size,
        bytes_per_sector,
    )
    .map_err(|e| DbError::System(format!("Failed to inspect NTFS $MFT runs: {}", e)))?;
    if mft_data_runs.len() > 1 {
        tracing::info!(
            "MFT reader: stitching fragmented $MFT from {} data runs",
            mft_data_runs.len()
        );
    }

    // --- Channel setup ---
    // reader → parser: raw MFT chunk buffers
    let (chunk_tx, chunk_rx): (Sender<MftChunk>, Receiver<MftChunk>) = bounded(MFT_CHANNEL_BOUND);
    // parser → writer: parsed FileEntry batches
    let (entry_tx, entry_rx): (Sender<Vec<FileEntry>>, Receiver<Vec<FileEntry>>) =
        bounded(MFT_CHANNEL_BOUND);

    let processed = Arc::new(AtomicU64::new(0));
    let errors = Arc::new(AtomicU64::new(0));

    // --- Reader thread ---
    let reader_path = e01_path.to_path_buf();
    let reader_processed = processed.clone();
    let reader_cancel = cancel.clone();
    let reader_mft_data_runs = mft_data_runs.clone();

    let reader_handle = thread::Builder::new()
        .name("mft-reader".into())
        .spawn(move || {
            // Each reader thread opens its own E01Reader
            let mut reader = match E01Reader::open(&reader_path) {
                Ok(r) => r,
                Err(e) => {
                    tracing::error!("MFT reader: failed to open E01: {}", e);
                    return;
                }
            };

            let mut start_record = 0u64;
            while start_record < total_records {
                // Check cancel
                if let Some(ref cancel) = reader_cancel {
                    if cancel.load(Ordering::Relaxed) {
                        tracing::info!("MFT reader: cancelled");
                        return;
                    }
                }

                let chunk_count = MFT_CHUNK_RECORDS.min(total_records - start_record);
                let byte_count = chunk_count * scanner_record_size as u64;
                let mft_stream_offset = start_record * scanner_record_size as u64;

                let mut buf = vec![0u8; byte_count as usize];
                let read_result = if reader_mft_data_runs.is_empty() {
                    read_contiguous_ntfs_mft_stream(
                        &mut reader,
                        volume_offset,
                        mft_cluster,
                        cluster_size,
                        mft_stream_offset,
                        &mut buf,
                    )
                } else {
                    read_ntfs_mft_stream(
                        &mut reader,
                        volume_offset,
                        cluster_size,
                        &reader_mft_data_runs,
                        mft_stream_offset,
                        &mut buf,
                    )
                };
                if let Err(e) = read_result {
                    tracing::warn!("MFT reader: read error at record {}: {}", start_record, e);
                    break;
                }

                let chunk = MftChunk {
                    data: buf,
                    start_record,
                    count: chunk_count,
                };

                if chunk_tx.send(chunk).is_err() {
                    break; // channel closed
                }

                start_record += chunk_count;
                reader_processed.store(start_record, Ordering::Relaxed);
            }
            // Drop chunk_tx to signal EOF to parsers
            drop(chunk_tx);
        })
        .map_err(|e| DbError::System(format!("Failed to spawn MFT reader: {}", e)))?;

    // --- Parser thread pool ---
    let num_parsers = num_cpus::get().clamp(2, 8);
    let mut parser_handles = Vec::with_capacity(num_parsers);

    for parser_id in 0..num_parsers {
        let rx = chunk_rx.clone();
        let tx = entry_tx.clone();
        let ds_id = data_source_id.clone();

        let handle = thread::Builder::new()
            .name(format!("mft-parser-{}", parser_id))
            .spawn(move || {
                let scanner = MftScanner::new(
                    volume_offset,
                    mft_cluster,
                    cluster_size,
                    scanner_record_size,
                    bytes_per_sector,
                    mft_data_size,
                );

                for chunk in rx.iter() {
                    let records = scanner.parse_chunk(&chunk.data, chunk.start_record, chunk.count);
                    let entries = records_to_file_entries(&records, &ds_id);
                    if !entries.is_empty() && tx.send(entries).is_err() {
                        break;
                    }
                }
            })
            .map_err(|e| DbError::System(format!("Failed to spawn MFT parser: {}", e)))?;

        parser_handles.push(handle);
    }

    // Drop our copy of chunk_rx and entry_tx so channels close properly
    drop(chunk_rx);
    drop(entry_tx);

    // --- Writer thread (runs on current thread for SQLite safety) ---
    let repo = FileRepo::new(conn);
    let mut total_files = 0u64;
    let mut total_dirs = 0u64;
    let mut total_size = 0u64;
    let mut warnings = Vec::new();
    let mut batch: Vec<FileEntry> = Vec::with_capacity(MFT_DB_BATCH_SIZE);
    let mut path_map: HashMap<String, (Option<String>, String, bool)> = HashMap::new();
    let mut deleted_records: HashSet<String> = HashSet::new();

    for entry_batch in entry_rx.iter() {
        for mut entry in entry_batch {
            match entry.entry_type {
                EntryType::File => {
                    total_files += 1;
                    total_size += entry.size.unwrap_or(0);
                }
                EntryType::Directory => total_dirs += 1,
            }

            add_entry_to_path_map(&mut path_map, &mut deleted_records, &entry);

            // MFT parents can appear later than their children. Insert with a
            // temporary null parent, then restore parent links after all rows
            // exist so SQLite self-referential foreign keys are satisfied.
            entry.parent_id = None;
            batch.push(entry);

            if batch.len() >= MFT_DB_BATCH_SIZE {
                if let Err(e) = repo.insert_batch(&batch) {
                    warnings.push(format!("DB insert error: {}", e));
                    errs_add(&errors);
                }
                batch.clear();
            }
        }

        // Progress
        let done = processed.load(Ordering::Relaxed);
        if let Some(pf) = progress_fn {
            let pct = ((done as f64 / total_records as f64) * 90.0) as u32;
            pf(
                5 + pct,
                &format!("Scanned {} / {} MFT records", done, total_records),
            );
        }
    }

    // Flush remaining batch
    if !batch.is_empty() {
        if let Err(e) = repo.insert_batch(&batch) {
            warnings.push(format!("DB insert error: {}", e));
        }
    }

    // Wait for reader and parsers
    if let Err(e) = reader_handle.join() {
        warnings.push(format!("MFT reader thread panicked: {:?}", e));
        tracing::error!("MFT reader thread panicked: {:?}", e);
    }
    for h in parser_handles {
        if let Err(e) = h.join() {
            warnings.push(format!("MFT parser thread panicked: {:?}", e));
            tracing::error!("MFT parser thread panicked: {:?}", e);
        }
    }

    if let Some(pf) = progress_fn {
        pf(95, "Reconstructing paths...");
    }

    update_entry_paths(conn, data_source_id, &path_map, &deleted_records)?;
    update_entry_parent_ids(conn, data_source_id, &path_map)?;

    if let Some(pf) = progress_fn {
        pf(100, "MFT scan complete");
    }

    Ok(EnumerationStats {
        file_count: total_files,
        dir_count: total_dirs,
        total_size,
        warnings,
    })
}

/// Read and parse the non-resident $DATA runlist from NTFS $MFT record 0.
fn read_ntfs_mft_data_runs(
    e01_path: &Path,
    volume_offset: u64,
    mft_cluster: u64,
    cluster_size: u64,
    record_size: u32,
    bytes_per_sector: u16,
) -> std::io::Result<Vec<(i64, u64)>> {
    let mut reader = E01Reader::open(e01_path)?;
    let mut record = vec![0u8; record_size as usize];
    read_contiguous_ntfs_mft_stream(
        &mut reader,
        volume_offset,
        mft_cluster,
        cluster_size,
        0,
        &mut record,
    )?;
    apply_ntfs_record_fixup(&mut record, bytes_per_sector as usize)?;
    parse_ntfs_mft_data_runs_from_record(&record)
}

fn apply_ntfs_record_fixup(record: &mut [u8], sector_size: usize) -> std::io::Result<()> {
    if record.len() < 8 || sector_size < 2 {
        return Ok(());
    }

    let usa_offset = u16::from_le_bytes([record[4], record[5]]) as usize;
    let usa_count = u16::from_le_bytes([record[6], record[7]]) as usize;
    if usa_offset == 0 || usa_count < 2 {
        return Ok(());
    }
    let usa_bytes = usa_count.checked_mul(2).ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidData, "invalid update sequence")
    })?;
    if usa_offset + usa_bytes > record.len() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "update sequence array exceeds record length",
        ));
    }

    let expected = [record[usa_offset], record[usa_offset + 1]];
    for index in 1..usa_count {
        let fixup_pos = index
            .checked_mul(sector_size)
            .and_then(|value| value.checked_sub(2))
            .ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::InvalidData, "invalid fixup position")
            })?;
        if fixup_pos + 2 > record.len() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "record too short for update sequence fixup",
            ));
        }
        if record[fixup_pos..fixup_pos + 2] != expected {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "update sequence signature mismatch",
            ));
        }

        let replacement = usa_offset + index * 2;
        record[fixup_pos] = record[replacement];
        record[fixup_pos + 1] = record[replacement + 1];
    }
    Ok(())
}

fn parse_ntfs_mft_data_runs_from_record(record: &[u8]) -> std::io::Result<Vec<(i64, u64)>> {
    if record.len() < 0x18 || &record[0..4] != b"FILE" {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "MFT record 0 is not a valid FILE record",
        ));
    }

    let attr_off = u16::from_le_bytes([record[0x14], record[0x15]]) as usize;
    let mut pos = attr_off;
    while pos + 8 < record.len() {
        let typ = u32::from_le_bytes(record[pos..pos + 4].try_into().map_err(|_| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Invalid MFT attribute type",
            )
        })?);
        if typ == 0xFFFFFFFF {
            break;
        }
        let len = u32::from_le_bytes(record[pos + 4..pos + 8].try_into().map_err(|_| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Invalid MFT attribute length",
            )
        })?) as usize;
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

fn parse_ntfs_data_runs(mut data: &[u8]) -> std::io::Result<Vec<(i64, u64)>> {
    const MAX_DATA_RUNS: usize = 100_000;

    let mut runs = Vec::new();
    let mut prev_lcn: i64 = 0;
    while !data.is_empty() && data[0] != 0 {
        if runs.len() >= MAX_DATA_RUNS {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("too many data runs (limit: {MAX_DATA_RUNS})"),
            ));
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

fn read_ntfs_mft_stream(
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

/// Read from the legacy contiguous $MFT layout used by this entry point.
fn read_contiguous_ntfs_mft_stream(
    reader: &mut dyn EvidenceReader,
    volume_offset: u64,
    mft_cluster: u64,
    cluster_size: u64,
    stream_offset: u64,
    out: &mut [u8],
) -> std::io::Result<()> {
    let mft_abs_offset = volume_offset
        .checked_add(mft_cluster.checked_mul(cluster_size).ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "MFT absolute offset overflow",
            )
        })?)
        .and_then(|base| base.checked_add(stream_offset))
        .ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, "MFT read offset overflow")
        })?;

    reader.seek(SeekFrom::Start(mft_abs_offset))?;
    reader.read_exact(out)
}

/// Internal chunk of raw MFT data.
struct MftChunk {
    data: Vec<u8>,
    start_record: u64,
    count: u64,
}

/// Convert parsed MFT records to domain FileEntry objects.
fn records_to_file_entries(records: &[MftRecord], data_source_id: &DataSourceId) -> Vec<FileEntry> {
    records
        .iter()
        .filter(|r| r.is_valid && (!r.name.is_empty() || r.record_number == 5))
        .map(|r| {
            let name = if r.record_number == 5 && (r.name.is_empty() || r.name == ".") {
                "\\".to_string()
            } else {
                r.name.clone()
            };
            let entry_type = if r.is_dir {
                EntryType::Directory
            } else {
                EntryType::File
            };
            let ext = if r.is_dir {
                None
            } else {
                r.name
                    .rsplit('.')
                    .next()
                    .filter(|e| *e != r.name)
                    .map(|e| e.to_string())
            };
            FileEntry {
                id: FileEntryId(format!("mft:{}", r.record_number)),
                parent_id: if r.record_number == 5 {
                    None
                } else {
                    Some(FileEntryId(format!("mft:{}", r.parent_ref)))
                },
                data_source_id: data_source_id.clone(),
                path: String::new(), // filled in during path reconstruction
                name,
                entry_type,
                size: if r.is_dir { None } else { Some(r.size) },
                ext,
                deleted: r.deleted,
                hidden: r.hidden
                    || visibility::inferred_hidden_name(&r.name)
                    || visibility::inferred_system_name(&r.name),
                system: r.system || visibility::inferred_system_name(&r.name),
                created_at: r.created_at,
                modified_at: r.modified_at,
                accessed_at: r.accessed_at,
                changed_at: r.changed_at,
                hash_sha256: None,
            }
        })
        .collect()
}

fn add_entry_to_path_map(
    path_map: &mut HashMap<String, (Option<String>, String, bool)>,
    deleted_records: &mut HashSet<String>,
    entry: &FileEntry,
) {
    let record_num = entry.id.0.strip_prefix("mft:").unwrap_or(&entry.id.0);
    let parent_num = entry
        .parent_id
        .as_ref()
        .and_then(|p| p.0.strip_prefix("mft:").map(|s| s.to_string()));
    path_map.insert(
        record_num.to_string(),
        (
            parent_num,
            entry.name.clone(),
            entry.entry_type == EntryType::Directory,
        ),
    );
    if entry.deleted {
        deleted_records.insert(record_num.to_string());
    }
}

/// Reconstruct full paths from parent_ref chains and update DB entries.
///
/// Uses recursive resolution with caching for O(n) complexity.
fn update_entry_paths(
    conn: &Connection,
    data_source_id: &DataSourceId,
    path_map: &HashMap<String, (Option<String>, String, bool)>,
    deleted_records: &HashSet<String>,
) -> DbResult<()> {
    let mut resolved: HashMap<String, String> = HashMap::with_capacity(path_map.len());
    let mut visiting: HashSet<String> = HashSet::new(); // Cycle detection

    // Recursive path resolution with caching
    fn resolve_path(
        record: &str,
        path_map: &HashMap<String, (Option<String>, String, bool)>,
        deleted_records: &HashSet<String>,
        resolved: &mut HashMap<String, String>,
        visiting: &mut HashSet<String>,
    ) -> String {
        // Already resolved
        if let Some(path) = resolved.get(record) {
            return path.clone();
        }

        // Cycle detection
        if !visiting.insert(record.to_string()) {
            tracing::warn!("Cycle detected in path chain at record {}", record);
            return String::new();
        }

        let (parent, name, _) = match path_map.get(record) {
            Some(entry) => entry,
            None => {
                visiting.remove(record);
                return String::new();
            }
        };

        let path = match parent {
            Some(p) if p != "5" && path_map.contains_key(p) => {
                let parent_path = resolve_path(p, path_map, deleted_records, resolved, visiting);
                if parent_path.is_empty() {
                    name.clone()
                } else {
                    format!("{}/{}", parent_path, name)
                }
            }
            _ if record != "5" && deleted_records.contains(record) => {
                format!("/$DeletedOrphans/{}-{}", record, name)
            }
            _ => name.clone(), // Root entry
        };

        resolved.insert(record.to_string(), path.clone());
        visiting.remove(record);
        path
    }

    // Resolve all entries
    let records: Vec<String> = path_map.keys().cloned().collect();
    for record in &records {
        resolve_path(
            record,
            path_map,
            deleted_records,
            &mut resolved,
            &mut visiting,
        );
    }

    // Update DB in batches
    let tx = conn.unchecked_transaction()?;
    {
        let mut stmt =
            tx.prepare("UPDATE file_entries SET path = ?1 WHERE id = ?2 AND data_source_id = ?3")?;
        for (record_num, path) in &resolved {
            let entry_id = format!("mft:{}", record_num);
            stmt.execute(rusqlite::params![path, entry_id, data_source_id.0])?;
        }
    }
    tx.commit()?;
    Ok(())
}

fn update_entry_parent_ids(
    conn: &Connection,
    data_source_id: &DataSourceId,
    path_map: &HashMap<String, (Option<String>, String, bool)>,
) -> DbResult<()> {
    let tx = conn.unchecked_transaction()?;
    {
        let mut stmt = tx.prepare(
            "UPDATE file_entries SET parent_id = ?1 WHERE id = ?2 AND data_source_id = ?3",
        )?;
        for (record_num, (parent, _, _)) in path_map {
            let entry_id = format!("mft:{}", record_num);
            let parent_id = mft_parent_entry_id(record_num, parent.as_deref(), path_map);
            stmt.execute(rusqlite::params![parent_id, entry_id, data_source_id.0])?;
        }
    }
    tx.commit()?;
    Ok(())
}

fn mft_parent_entry_id(
    record_num: &str,
    parent_num: Option<&str>,
    path_map: &HashMap<String, (Option<String>, String, bool)>,
) -> Option<String> {
    if record_num == "5" {
        return None;
    }

    match parent_num {
        Some(parent) if parent != record_num && path_map.contains_key(parent) => {
            Some(format!("mft:{}", parent))
        }
        _ if path_map.contains_key("5") => Some("mft:5".to_string()),
        _ => None,
    }
}

fn errs_add(errors: &Arc<AtomicU64>) {
    errors.fetch_add(1, Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;
    use evidence_core::{filesystem::root_node, FsNode};
    use persistence_sqlite::{open_or_create, runner};
    use rusqlite::params;
    use std::{
        cmp::Ordering as CmpOrdering,
        io::{self, Cursor, Read, Seek},
        path::PathBuf,
        sync::atomic::AtomicBool,
    };
    use tempfile::TempDir;

    use crate::file_service::{
        partition_roots::{
            looks_like_raw_fs_root_name, mft_entry_partition_index, normalized_bare_root_name,
            partition_placeholder_index, partition_placeholder_status,
        },
        sort::{natural_cmp, sort_entries},
    };
    use evidence_core::FileSystemReader;
    use transport::commands::{FileSortDirectionDto, FileSortKeyDto, GetFileRowsRequest};

    struct CancelAfterRootFs;

    impl FileSystemReader for CancelAfterRootFs {
        fn root(&self) -> io::Result<FsNode> {
            Ok(root_node())
        }

        fn list_children(&self, path: &str) -> io::Result<Vec<FsNode>> {
            if path.is_empty() {
                Ok(vec![
                    FsNode {
                        name: "first.txt".to_string(),
                        path: "first.txt".to_string(),
                        is_dir: false,
                        size: 1,
                        hidden: false,
                        system: false,
                        created_at: None,
                        modified_at: None,
                        accessed_at: None,
                    },
                    FsNode {
                        name: "second.txt".to_string(),
                        path: "second.txt".to_string(),
                        is_dir: false,
                        size: 1,
                        hidden: false,
                        system: false,
                        created_at: None,
                        modified_at: None,
                        accessed_at: None,
                    },
                ])
            } else {
                Ok(Vec::new())
            }
        }

        fn open_file(&self, _path: &str) -> io::Result<Box<dyn Read>> {
            Ok(Box::new(Cursor::new(Vec::<u8>::new())))
        }

        fn data_source_name(&self) -> &str {
            "cancel-after-root"
        }
    }

    #[test]
    fn enumerate_filesystem_cancel_rolls_back_transaction() {
        let conn = persistence_sqlite::connection::open_in_memory().unwrap();
        conn.execute_batch(include_str!(
            "../../../persistence-sqlite/src/migrations/scripts/0003_file_entries.sql"
        ))
        .unwrap();
        conn.execute_batch(include_str!(
            "../../../persistence-sqlite/src/migrations/scripts/0022_file_entry_visibility_flags.sql"
        ))
        .unwrap();
        let cancel = AtomicBool::new(false);
        let ds_id = DataSourceId("ds-cancel-enum".to_string());
        let fs = CancelAfterRootFs;

        let Err(err) = enumerate_filesystem_with_root_name_and_cancel(
            &conn,
            &ds_id,
            &fs,
            None,
            Some(&|_| cancel.store(true, Ordering::Relaxed)),
            Some(&cancel),
        ) else {
            panic!("expected cancellation to roll back enumeration transaction");
        };

        assert!(err.to_string().contains("Enumeration cancelled"));
        let row_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM file_entries", [], |row| row.get(0))
            .unwrap();
        assert_eq!(row_count, 0);
    }

    #[test]
    fn safe_path_rejects_dot_dot_traversal() {
        assert!(safe_relative_path("../etc/passwd").is_err());
        assert!(safe_relative_path("foo/../../bar").is_err());
        assert!(safe_relative_path("..\\windows\\system32").is_err());
    }

    #[test]
    fn safe_path_rejects_url_encoded_traversal() {
        assert!(safe_relative_path("%2e%2e%2fetc%2fpasswd").is_err());
        assert!(safe_relative_path("foo%2f%2e%2e%2fbar").is_err());
    }

    #[test]
    fn safe_path_rejects_null_byte() {
        assert!(safe_relative_path("file.txt\0.jpg").is_err());
    }

    #[test]
    fn safe_path_rejects_absolute_path() {
        assert!(safe_relative_path("/etc/passwd").is_err());
    }

    #[test]
    fn safe_path_accepts_valid_paths() {
        assert!(safe_relative_path("documents/file.txt").is_ok());
        assert!(safe_relative_path("a/b/c.txt").is_ok());
        assert!(safe_relative_path("simple.txt").is_ok());
    }

    #[test]
    fn safe_path_rejects_empty_path() {
        assert!(safe_relative_path("").is_err());
    }

    #[test]
    fn safe_path_rejects_windows_reserved_names() {
        assert!(safe_relative_path("CON").is_err());
        assert!(safe_relative_path("NUL.txt").is_err());
        assert!(safe_relative_path("COM1").is_err());
        assert!(safe_relative_path("LPT1.dat").is_err());
    }

    #[test]
    fn mft_root_record_becomes_tree_root() {
        let records = vec![
            MftRecord {
                record_number: 5,
                name: ".".to_string(),
                parent_ref: 5,
                is_dir: true,
                size: 0,
                created_at: None,
                modified_at: None,
                accessed_at: None,
                changed_at: None,
                hidden: false,
                system: false,
                deleted: false,
                is_valid: true,
            },
            MftRecord {
                record_number: 42,
                name: "Windows".to_string(),
                parent_ref: 5,
                is_dir: true,
                size: 0,
                created_at: None,
                modified_at: None,
                accessed_at: None,
                changed_at: None,
                hidden: false,
                system: false,
                deleted: false,
                is_valid: true,
            },
        ];

        let entries = records_to_file_entries(&records, &DataSourceId("ds".to_string()));

        let root = entries
            .iter()
            .find(|entry| entry.id.0 == "mft:5")
            .expect("root MFT record should be retained");
        assert_eq!(root.name, "\\");
        assert!(root.parent_id.is_none());

        let child = entries
            .iter()
            .find(|entry| entry.id.0 == "mft:42")
            .expect("child MFT record should be retained");
        assert_eq!(
            child.parent_id.as_ref().map(|id| id.0.as_str()),
            Some("mft:5")
        );
    }

    #[test]
    fn mft_orphan_records_are_anchored_to_root() {
        let mut path_map = HashMap::new();
        path_map.insert("5".to_string(), (None, "\\".to_string(), true));
        path_map.insert(
            "42".to_string(),
            (Some("999".to_string()), "Orphan".to_string(), true),
        );

        assert_eq!(
            mft_parent_entry_id("42", Some("999"), &path_map),
            Some("mft:5".to_string())
        );
        assert_eq!(mft_parent_entry_id("5", None, &path_map), None);
    }

    #[test]
    fn mft_deleted_orphan_path_uses_deleted_orphans_prefix() {
        let conn = persistence_sqlite::connection::open_in_memory().unwrap();
        conn.execute_batch(include_str!(
            "../../../persistence-sqlite/src/migrations/scripts/0003_file_entries.sql"
        ))
        .unwrap();
        conn.execute_batch(include_str!(
            "../../../persistence-sqlite/src/migrations/scripts/0022_file_entry_visibility_flags.sql"
        ))
        .unwrap();
        let ds_id = DataSourceId("ds-deleted-orphan".to_string());
        let mut entries = records_to_file_entries(
            &[
                MftRecord {
                    record_number: 5,
                    name: ".".to_string(),
                    parent_ref: 5,
                    is_dir: true,
                    size: 0,
                    created_at: None,
                    modified_at: None,
                    accessed_at: None,
                    changed_at: None,
                    hidden: false,
                    system: false,
                    deleted: false,
                    is_valid: true,
                },
                MftRecord {
                    record_number: 77,
                    name: "old.txt".to_string(),
                    parent_ref: 999,
                    is_dir: false,
                    size: 12,
                    created_at: None,
                    modified_at: None,
                    accessed_at: None,
                    changed_at: None,
                    hidden: false,
                    system: false,
                    deleted: true,
                    is_valid: true,
                },
            ],
            &ds_id,
        );
        for entry in &mut entries {
            entry.parent_id = None;
        }
        FileRepo::new(&conn).insert_batch(&entries).unwrap();
        let mut path_map = HashMap::new();
        let mut deleted_records = HashSet::new();
        for entry in &entries {
            add_entry_to_path_map(&mut path_map, &mut deleted_records, entry);
        }

        update_entry_paths(&conn, &ds_id, &path_map, &deleted_records).unwrap();
        update_entry_parent_ids(&conn, &ds_id, &path_map).unwrap();

        let (path, parent_id, deleted): (String, Option<String>, i32) = conn
            .query_row(
                "SELECT path, parent_id, deleted FROM file_entries WHERE id = 'mft:77'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(path, "/$DeletedOrphans/77-old.txt");
        assert_eq!(parent_id.as_deref(), Some("mft:5"));
        assert_eq!(deleted, 1);
    }

    #[test]
    fn mft_partition_prefixed_id_exposes_partition_index() {
        assert_eq!(
            crate::file_service::viewer::mft_partition_index_from_entry_id("mft:3:42"),
            Some(3)
        );
        assert_eq!(
            crate::file_service::viewer::mft_partition_index_from_entry_id("mft:42"),
            None
        );
        assert_eq!(
            crate::file_service::viewer::mft_partition_index_from_entry_id("uuid"),
            None
        );
    }

    struct SliceEvidenceReader {
        data: Vec<u8>,
        pos: u64,
        info: evidence_core::ReaderInfo,
    }

    impl SliceEvidenceReader {
        fn new(data: Vec<u8>) -> Self {
            Self {
                data,
                pos: 0,
                info: evidence_core::ReaderInfo {
                    path: PathBuf::from("slice"),
                    size: 0,
                    kind: "test".to_string(),
                },
            }
        }
    }

    impl Read for SliceEvidenceReader {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            let start = self.pos as usize;
            if start >= self.data.len() {
                return Ok(0);
            }
            let count = buf.len().min(self.data.len() - start);
            buf[..count].copy_from_slice(&self.data[start..start + count]);
            self.pos += count as u64;
            Ok(count)
        }
    }

    impl Seek for SliceEvidenceReader {
        fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
            let next = match pos {
                SeekFrom::Start(value) => value as i128,
                SeekFrom::End(value) => self.data.len() as i128 + value as i128,
                SeekFrom::Current(value) => self.pos as i128 + value as i128,
            };
            if next < 0 {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "negative seek",
                ));
            }
            self.pos = next as u64;
            Ok(self.pos)
        }
    }

    impl EvidenceReader for SliceEvidenceReader {
        fn info(&self) -> &evidence_core::ReaderInfo {
            &self.info
        }
    }

    #[test]
    fn read_ntfs_mft_stream_stitches_fragmented_runs() {
        let mut disk = vec![0u8; 4096];
        disk[1024..1536].fill(b'A');
        disk[3072..3584].fill(b'B');
        let mut reader = SliceEvidenceReader::new(disk);
        let mut out = vec![0u8; 1024];

        read_ntfs_mft_stream(&mut reader, 0, 512, &[(2, 1), (6, 1)], 0, &mut out).unwrap();

        assert!(out[..512].iter().all(|byte| *byte == b'A'));
        assert!(out[512..].iter().all(|byte| *byte == b'B'));
    }

    #[test]
    fn read_ntfs_mft_stream_stitches_read_crossing_run_boundary() {
        let mut disk = vec![0u8; 4096];
        disk[1024..1536].fill(b'A');
        disk[3072..3584].fill(b'B');
        let mut reader = SliceEvidenceReader::new(disk);
        let mut out = vec![0u8; 512];

        read_ntfs_mft_stream(&mut reader, 0, 512, &[(2, 1), (6, 1)], 256, &mut out).unwrap();

        assert!(out[..256].iter().all(|byte| *byte == b'A'));
        assert!(out[256..].iter().all(|byte| *byte == b'B'));
    }

    #[test]
    fn read_ntfs_mft_stream_rejects_negative_lcn() {
        let mut reader = SliceEvidenceReader::new(vec![0u8; 1024]);
        let mut out = vec![0u8; 512];

        let err = read_ntfs_mft_stream(&mut reader, 0, 512, &[(-1, 1)], 0, &mut out)
            .expect_err("negative LCN must fail closed");

        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    }

    #[test]
    fn parse_ntfs_data_runs_decodes_fragmented_runs() {
        let runs = parse_ntfs_data_runs(&[0x11, 0x02, 0x05, 0x11, 0x03, 0x07, 0x00]).unwrap();

        assert_eq!(runs, vec![(5, 2), (12, 3)]);
    }

    #[test]
    fn safe_path_allows_normal_names() {
        assert!(safe_relative_path("config.txt").is_ok());
        assert!(safe_relative_path("data.json").is_ok());
        assert!(safe_relative_path("folder/subfolder/file.log").is_ok());
    }

    // ------------------------------------------------------------------
    // Service-layer sort comparator
    // ------------------------------------------------------------------

    fn sort_entry(
        name: &str,
        entry_type: EntryType,
        hidden: bool,
        system: bool,
        deleted: bool,
        size: Option<u64>,
    ) -> FileEntry {
        FileEntry {
            id: FileEntryId(format!("id-{name}")),
            parent_id: None,
            data_source_id: DataSourceId("ds".to_string()),
            path: name.to_string(),
            name: name.to_string(),
            entry_type,
            size,
            ext: name
                .rsplit('.')
                .next()
                .filter(|e| *e != name)
                .map(|e| e.to_string()),
            deleted,
            hidden,
            system,
            created_at: None,
            modified_at: None,
            accessed_at: None,
            changed_at: None,
            hash_sha256: None,
        }
    }

    fn names_after_sort(
        mut entries: Vec<FileEntry>,
        key: FileSortKeyDto,
        dir: FileSortDirectionDto,
    ) -> Vec<String> {
        sort_entries(&mut entries, key, dir);
        entries.into_iter().map(|e| e.name).collect()
    }

    #[test]
    fn natural_sort_orders_numeric_suffixes_like_explorer() {
        assert_eq!(natural_cmp("file2", "file10"), CmpOrdering::Less);
        assert_eq!(natural_cmp("file10", "file2"), CmpOrdering::Greater);
        assert_eq!(natural_cmp("img9", "img09"), CmpOrdering::Less); // equal magnitude, shorter raw run first
        assert_eq!(natural_cmp("Alpha", "alpha"), CmpOrdering::Less); // case-insensitive then raw
    }

    #[test]
    fn sort_keeps_directories_before_files_even_when_descending() {
        let entries = vec![
            sort_entry("zeta.txt", EntryType::File, false, false, false, Some(1)),
            sort_entry("alpha", EntryType::Directory, false, false, false, None),
            sort_entry("beta", EntryType::Directory, false, false, false, None),
        ];
        let ordered = names_after_sort(entries, FileSortKeyDto::Name, FileSortDirectionDto::Desc);
        // Directories first (fixed), files after — even under descending name sort.
        assert_eq!(ordered, vec!["beta", "alpha", "zeta.txt"]);
    }

    #[test]
    fn sort_uses_natural_name_order_for_files() {
        let entries = vec![
            sort_entry("file10.log", EntryType::File, false, false, false, Some(1)),
            sort_entry("file2.log", EntryType::File, false, false, false, Some(1)),
            sort_entry("file1.log", EntryType::File, false, false, false, Some(1)),
        ];
        let ordered = names_after_sort(entries, FileSortKeyDto::Name, FileSortDirectionDto::Asc);
        assert_eq!(ordered, vec!["file1.log", "file2.log", "file10.log"]);
    }

    #[test]
    fn sort_sinks_hidden_system_deleted_after_normal() {
        let entries = vec![
            sort_entry("normal.txt", EntryType::File, false, false, false, Some(1)),
            sort_entry("deleted.txt", EntryType::File, false, false, true, Some(1)),
            sort_entry("hidden.txt", EntryType::File, true, false, false, Some(1)),
            sort_entry("both.txt", EntryType::File, true, false, true, Some(1)),
        ];
        let ordered = names_after_sort(entries, FileSortKeyDto::Name, FileSortDirectionDto::Asc);
        // Buckets: normal(0) < hidden/system(1) < deleted(2) < hidden+deleted(3).
        assert_eq!(
            ordered,
            vec!["normal.txt", "hidden.txt", "deleted.txt", "both.txt"]
        );
    }

    #[test]
    fn sort_status_buckets_are_fixed_under_descending_name() {
        let entries = vec![
            sort_entry("aaa.txt", EntryType::File, true, false, false, Some(1)), // hidden
            sort_entry("zzz.txt", EntryType::File, false, false, false, Some(1)), // normal
        ];
        let ordered = names_after_sort(entries, FileSortKeyDto::Name, FileSortDirectionDto::Desc);
        // Normal bucket still precedes hidden bucket regardless of direction.
        assert_eq!(ordered, vec!["zzz.txt", "aaa.txt"]);
    }

    #[test]
    fn sort_by_size_descending_within_files() {
        let entries = vec![
            sort_entry("small.bin", EntryType::File, false, false, false, Some(10)),
            sort_entry("big.bin", EntryType::File, false, false, false, Some(9000)),
            sort_entry("mid.bin", EntryType::File, false, false, false, Some(500)),
        ];
        let ordered = names_after_sort(entries, FileSortKeyDto::Size, FileSortDirectionDto::Desc);
        assert_eq!(ordered, vec!["big.bin", "mid.bin", "small.bin"]);
    }

    #[test]
    fn get_file_rows_sorts_full_set_then_paginates() {
        let tmp = TempDir::new().unwrap();
        let conn = open_or_create(&tmp.path().join("case.db")).unwrap();
        runner::run_all(&conn).unwrap();
        let ds_id = DataSourceId("ds-sort-page".to_string());
        conn.execute(
            "INSERT INTO cases (id, name, created_at, updated_at) VALUES ('c1','C','2026-01-01T00:00:00Z','2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO data_sources (id, case_id, name, kind, source_path, imported_at) VALUES (?1,'c1','ds','logical_directory','C:/e','2026-01-01T00:00:00Z')",
            params![ds_id.0],
        )
        .unwrap();

        // Root parent dir + children: 1 dir + files file1,file2,file10.
        let parent = FileEntryId("parent".to_string());
        let repo = FileRepo::new(&conn);
        let mut parent_entry = sort_entry("root", EntryType::Directory, false, false, false, None);
        parent_entry.id = parent.clone();
        parent_entry.data_source_id = ds_id.clone();
        repo.insert_batch(&[parent_entry]).unwrap();

        let mut children = Vec::new();
        for n in ["sub", "file10.txt", "file2.txt", "file1.txt"] {
            let is_dir = n == "sub";
            let mut child = sort_entry(
                n,
                if is_dir {
                    EntryType::Directory
                } else {
                    EntryType::File
                },
                false,
                false,
                false,
                if is_dir { None } else { Some(1) },
            );
            child.id = FileEntryId(format!("c-{n}"));
            child.parent_id = Some(parent.clone());
            child.data_source_id = ds_id.clone();
            children.push(child);
        }
        repo.insert_batch(&children).unwrap();

        let request = GetFileRowsRequest {
            parent_id: Some(parent.0.clone()),
            offset: 0,
            limit: 2,
            show_hidden: false,
            sort_key: FileSortKeyDto::Name,
            sort_direction: FileSortDirectionDto::Asc,
        };
        let page = get_file_rows_for_request(&conn, &request).unwrap();
        assert_eq!(page.total_count, 4);
        assert!(page.truncated);
        // Page 1: directory first, then natural-sorted file1.
        let names: Vec<_> = page.rows.iter().map(|r| r.name.clone()).collect();
        assert_eq!(names, vec!["sub", "file1.txt"]);

        let request2 = GetFileRowsRequest {
            offset: 2,
            ..request
        };
        let page2 = get_file_rows_for_request(&conn, &request2).unwrap();
        let names2: Vec<_> = page2.rows.iter().map(|r| r.name.clone()).collect();
        assert_eq!(names2, vec!["file2.txt", "file10.txt"]);
    }

    // ------------------------------------------------------------------
    // Stage A: partition placeholder identity binding (index-encoded path)
    // ------------------------------------------------------------------

    fn placeholder_entry(path: &str) -> FileEntry {
        let mut entry = sort_entry(
            "Partition 1 (NTFS)",
            EntryType::Directory,
            false,
            false,
            false,
            None,
        );
        entry.path = path.to_string();
        entry
    }

    #[test]
    fn placeholder_path_encodes_partition_index() {
        let tmp = TempDir::new().unwrap();
        let conn = open_or_create(&tmp.path().join("case.db")).unwrap();
        runner::run_all(&conn).unwrap();
        let ds_id = DataSourceId("ds-ph-index".to_string());
        conn.execute(
            "INSERT INTO cases (id, name, created_at, updated_at) VALUES ('c1','C','2026-01-01T00:00:00Z','2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO data_sources (id, case_id, name, kind, source_path, imported_at) VALUES (?1,'c1','ds','e01','C:/e','2026-01-01T00:00:00Z')",
            params![ds_id.0],
        )
        .unwrap();

        let id =
            insert_partition_placeholder_root(&conn, &ds_id, 3, "Partition 3 (NTFS)", "queued")
                .unwrap();
        let entry = FileRepo::new(&conn).find_by_id(&id).unwrap().unwrap();
        assert_eq!(entry.path, "__partition_placeholder__/3/queued");
        assert_eq!(partition_placeholder_index(&entry), Some(3));
        assert_eq!(partition_placeholder_status(&entry), Some("queued"));
    }

    #[test]
    fn legacy_placeholder_path_without_index_still_parses_status() {
        // Old form: "__partition_placeholder__/{status}" (no index segment).
        let entry = placeholder_entry("__partition_placeholder__/locked");
        assert_eq!(partition_placeholder_index(&entry), None);
        assert_eq!(partition_placeholder_status(&entry), Some("locked"));
    }

    #[test]
    fn placeholder_index_distinguishes_same_named_partitions() {
        // Two placeholders with identical display names but different indices
        // must resolve to distinct identities via the path index segment.
        let p1 = placeholder_entry("__partition_placeholder__/1/queued");
        let p2 = placeholder_entry("__partition_placeholder__/2/queued");
        assert_eq!(partition_placeholder_index(&p1), Some(1));
        assert_eq!(partition_placeholder_index(&p2), Some(2));
        assert_ne!(
            partition_placeholder_index(&p1),
            partition_placeholder_index(&p2)
        );
    }

    #[test]
    fn placeholder_status_parses_multi_digit_index() {
        let entry = placeholder_entry("__partition_placeholder__/12/unsupported");
        assert_eq!(partition_placeholder_index(&entry), Some(12));
        assert_eq!(partition_placeholder_status(&entry), Some("unsupported"));
    }

    // ------------------------------------------------------------------
    // Stage C: read-side defensive root normalization
    // ------------------------------------------------------------------

    #[test]
    fn raw_fs_root_name_detection() {
        assert!(looks_like_raw_fs_root_name("\\"));
        assert!(looks_like_raw_fs_root_name("/"));
        assert!(looks_like_raw_fs_root_name("."));
        assert!(!looks_like_raw_fs_root_name("Windows"));
        assert!(!looks_like_raw_fs_root_name("EFI"));
        assert!(!looks_like_raw_fs_root_name("Partition 0 (NTFS)"));
    }

    #[test]
    fn mft_entry_partition_index_parsing() {
        assert_eq!(mft_entry_partition_index("mft:3:5"), Some(3));
        assert_eq!(mft_entry_partition_index("mft:0:42"), Some(0));
        assert_eq!(mft_entry_partition_index("mft:5"), None); // legacy, no partition
        assert_eq!(mft_entry_partition_index("uuid-abc"), None);
    }

    fn seed_ds_with_partition(conn: &Connection, ds_id: &str, index: u32, kind: &str) {
        conn.execute(
            "INSERT OR IGNORE INTO cases (id, name, created_at, updated_at) VALUES ('c1','C','2026-01-01T00:00:00Z','2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT OR IGNORE INTO data_sources (id, case_id, name, kind, source_path, imported_at) VALUES (?1,'c1','ds','e01','C:/e','2026-01-01T00:00:00Z')",
            params![ds_id],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO data_source_partitions (id, data_source_id, partition_index, name, kind_label, status, offset, length)
             VALUES (?1, ?2, ?3, ?4, ?5, 'supported', 0, 1024)",
            params![format!("p-{index}"), ds_id, index, format!("Part {index}"), kind],
        )
        .unwrap();
    }

    #[test]
    fn bare_root_renamed_via_mft_partition_index() {
        let tmp = TempDir::new().unwrap();
        let conn = open_or_create(&tmp.path().join("case.db")).unwrap();
        runner::run_all(&conn).unwrap();
        let ds_id = "ds-bare-mft";
        seed_ds_with_partition(&conn, ds_id, 3, "NTFS");

        let mut entry = sort_entry("\\", EntryType::Directory, false, false, false, None);
        entry.id = FileEntryId("mft:3:5".to_string());
        entry.data_source_id = DataSourceId(ds_id.to_string());

        assert_eq!(
            normalized_bare_root_name(&conn, &entry),
            "Partition 3 (NTFS)"
        );
    }

    #[test]
    fn bare_root_renamed_via_sole_partition_when_no_mft_index() {
        let tmp = TempDir::new().unwrap();
        let conn = open_or_create(&tmp.path().join("case.db")).unwrap();
        runner::run_all(&conn).unwrap();
        let ds_id = "ds-bare-sole";
        seed_ds_with_partition(&conn, ds_id, 0, "FAT");

        let mut entry = sort_entry("/", EntryType::Directory, false, false, false, None);
        entry.id = FileEntryId("uuid-root".to_string());
        entry.data_source_id = DataSourceId(ds_id.to_string());

        assert_eq!(
            normalized_bare_root_name(&conn, &entry),
            "Partition 0 (FAT)"
        );
    }

    #[test]
    fn bare_root_unknown_when_unattributable() {
        let tmp = TempDir::new().unwrap();
        let conn = open_or_create(&tmp.path().join("case.db")).unwrap();
        runner::run_all(&conn).unwrap();
        let ds_id = "ds-bare-unknown";
        // Two partitions, non-MFT id → cannot attribute deterministically.
        seed_ds_with_partition(&conn, ds_id, 0, "NTFS");
        seed_ds_with_partition(&conn, ds_id, 1, "FAT");

        let mut entry = sort_entry("\\", EntryType::Directory, false, false, false, None);
        entry.id = FileEntryId("uuid-ambiguous".to_string());
        entry.data_source_id = DataSourceId(ds_id.to_string());

        assert_eq!(
            normalized_bare_root_name(&conn, &entry),
            "Partition ? (UNKNOWN)"
        );
    }

    #[test]
    fn tree_builder_normalizes_residual_bare_root() {
        let tmp = TempDir::new().unwrap();
        let conn = open_or_create(&tmp.path().join("case.db")).unwrap();
        runner::run_all(&conn).unwrap();
        let ds_id = "ds-tree-bare";
        seed_ds_with_partition(&conn, ds_id, 2, "NTFS");

        // A residual bare `\` root directly in the main DB (simulates an older
        // case that escaped staging folding).
        conn.execute(
            "INSERT INTO file_entries (id, parent_id, data_source_id, path, name, entry_type, size)
             VALUES ('mft:2:5', NULL, ?1, '', '\\', 'directory', 0)",
            params![ds_id],
        )
        .unwrap();

        let tree = get_file_tree_real_with_visibility(&conn, false).unwrap();
        assert_eq!(tree.len(), 1);
        assert_eq!(tree[0].name, "Partition 2 (NTFS)");
        assert_eq!(tree[0].node_type.as_deref(), Some("partition"));
        assert!(!tree.iter().any(|n| n.name == "\\"));
    }
}
