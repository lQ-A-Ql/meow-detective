use super::{enumeration::EnumerationStats, visibility};
use chrono::Utc;
use domain::{
    DataSourceId, EdgeType, EntryType, FileEntry, FileEntryId, GraphEdge, GraphNode, NodeType,
};
use evidence_core::EvidenceReader;
use image_e01::E01Reader;
use persistence_sqlite::{
    repositories::{file_repo::FileRepo, graph_repo::GraphRepo},
    DbError, DbResult,
};
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

    // Populate investigative graph: File nodes and Contains edges
    if let Err(e) = populate_mft_file_graph(conn, data_source_id) {
        warnings.push(format!("Graph population warning: {}", e));
        tracing::warn!("Failed to populate file graph after MFT enumeration: {}", e);
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

pub fn parse_ntfs_data_runs(mut data: &[u8]) -> std::io::Result<Vec<(i64, u64)>> {
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

pub fn read_ntfs_mft_stream(
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
pub fn records_to_file_entries(
    records: &[MftRecord],
    data_source_id: &DataSourceId,
) -> Vec<FileEntry> {
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
                encrypted: false,
                created_at: r.created_at,
                modified_at: r.modified_at,
                accessed_at: r.accessed_at,
                changed_at: r.changed_at,
                hash_sha256: None,
            }
        })
        .collect()
}

pub fn add_entry_to_path_map(
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
pub fn update_entry_paths(
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

pub fn update_entry_parent_ids(
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

pub fn mft_parent_entry_id(
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

/// Write File graph nodes and Contains edges for all file entries belonging to
/// the given data source. Run after MFT enumeration completes so paths and
/// parent links are already persisted.
fn populate_mft_file_graph(conn: &Connection, data_source_id: &DataSourceId) -> DbResult<()> {
    let case_id: String = conn.query_row(
        "SELECT case_id FROM data_sources WHERE id = ?1",
        rusqlite::params![data_source_id.0],
        |row| row.get(0),
    )?;

    let graph_repo = GraphRepo::new(conn);
    let now = Utc::now().to_rfc3339();

    const GRAPH_QUERY_BATCH: u32 = 5000;
    const GRAPH_WRITE_CHUNK: usize = 2000;
    let mut offset = 0u64;

    loop {
        let mut stmt = conn.prepare(
            "SELECT id, parent_id, name, path, entry_type FROM file_entries
             WHERE data_source_id = ?1
             LIMIT ?2 OFFSET ?3",
        )?;
        let rows: Vec<(String, Option<String>, String, String, String)> = stmt
            .query_map(
                rusqlite::params![data_source_id.0, GRAPH_QUERY_BATCH, offset],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                },
            )?
            .collect::<Result<Vec<_>, _>>()?;

        if rows.is_empty() {
            break;
        }

        let row_count = rows.len() as u64;

        // Build nodes and edges from the query result
        let mut nodes: Vec<GraphNode> = Vec::with_capacity(rows.len());
        let mut edges: Vec<GraphEdge> = Vec::with_capacity(rows.len());

        for (id, parent_id, name, path, _entry_type) in &rows {
            // Only directories and files: skip root sentinels with empty names
            if name.is_empty() && parent_id.is_none() {
                continue;
            }

            nodes.push(GraphNode {
                id: id.clone(),
                case_id: case_id.clone(),
                node_type: NodeType::File,
                label: name.clone(),
                summary: path.clone(),
                tags: Vec::new(),
                created_at: now.clone(),
            });

            if let Some(pid) = parent_id {
                edges.push(GraphEdge {
                    id: format!("contains:{pid}:{id}"),
                    case_id: case_id.clone(),
                    source_id: pid.clone(),
                    target_id: id.clone(),
                    edge_type: EdgeType::Contains,
                    confidence: None,
                    provenance: None,
                    created_at: now.clone(),
                });
            }
        }

        // Write in chunks so each GraphRepo transaction stays bounded
        for node_chunk in nodes.chunks(GRAPH_WRITE_CHUNK) {
            graph_repo.insert_nodes_batch(node_chunk)?;
        }
        for edge_chunk in edges.chunks(GRAPH_WRITE_CHUNK) {
            graph_repo.insert_edges_batch(edge_chunk)?;
        }

        offset += row_count;
    }

    Ok(())
}
