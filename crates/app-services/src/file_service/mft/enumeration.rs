use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
    thread::JoinHandle,
};

use crossbeam_channel::{bounded, Receiver, Sender};
use domain::{DataSourceId, EntryType, FileEntry};
use fs_ntfs::mft_scanner::MftScanner;
use image_e01::E01Reader;
use persistence_sqlite::{repositories::file_repo::FileRepo, DbError, DbResult};
use rusqlite::Connection;

use crate::file_service::enumeration::EnumerationStats;

use super::{
    graph::populate_file_graph_for_data_source,
    records::{
        add_entry_to_path_map, records_to_file_entries, update_entry_parent_ids, update_entry_paths,
    },
    stream::{read_contiguous_ntfs_mft_stream, read_ntfs_mft_data_runs, read_ntfs_mft_stream},
};

const MFT_CHUNK_RECORDS: u64 = 10_000;
const MFT_CHANNEL_BOUND: usize = 4;
const MFT_DB_BATCH_SIZE: usize = 2_000;
type ProgressCallback<'a> = Option<&'a dyn Fn(u32, &str)>;

struct MftChunk {
    data: Vec<u8>,
    start_record: u64,
    count: u64,
}

#[derive(Clone)]
struct ScanConfig {
    e01_path: PathBuf,
    volume_offset: u64,
    mft_cluster: u64,
    cluster_size: u64,
    bytes_per_sector: u16,
    mft_data_size: u64,
    total_records: u64,
    scanner_record_size: u32,
    data_runs: Vec<(i64, u64)>,
}

struct ScanOutput {
    file_count: u64,
    dir_count: u64,
    total_size: u64,
    warnings: Vec<String>,
    path_map: HashMap<String, (Option<String>, String, bool)>,
    deleted_records: HashSet<String>,
}

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
    progress_fn: ProgressCallback<'_>,
    cancel: Option<Arc<AtomicBool>>,
) -> DbResult<EnumerationStats> {
    enumerate_filesystem_mft_with_partition(
        conn,
        data_source_id,
        e01_path,
        volume_offset,
        mft_cluster,
        cluster_size,
        record_size,
        bytes_per_sector,
        mft_data_size,
        progress_fn,
        cancel,
        0,
    )
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn enumerate_filesystem_mft_with_partition(
    conn: &Connection,
    data_source_id: &DataSourceId,
    e01_path: &Path,
    volume_offset: u64,
    mft_cluster: u64,
    cluster_size: u64,
    record_size: u32,
    bytes_per_sector: u16,
    mft_data_size: u64,
    progress_fn: ProgressCallback<'_>,
    cancel: Option<Arc<AtomicBool>>,
    partition_index: usize,
) -> DbResult<EnumerationStats> {
    if let Some(progress) = progress_fn {
        progress(5, "Starting MFT scan...");
    }
    let config = build_scan_config(
        e01_path,
        volume_offset,
        mft_cluster,
        cluster_size,
        record_size,
        bytes_per_sector,
        mft_data_size,
    )?;
    let mut output = run_scan(conn, data_source_id, &config, progress_fn, cancel)?;
    finalize_scan(
        conn,
        data_source_id,
        &output.path_map,
        &output.deleted_records,
        partition_index,
        progress_fn,
    )?;
    if let Err(error) = populate_file_graph_for_data_source(conn, data_source_id) {
        output
            .warnings
            .push(format!("Graph population warning: {error}"));
        tracing::warn!(
            "Failed to populate file graph after MFT enumeration: {}",
            error
        );
    }
    Ok(EnumerationStats {
        file_count: output.file_count,
        dir_count: output.dir_count,
        total_size: output.total_size,
        warnings: output.warnings,
        diagnostics: Vec::new(),
    })
}

#[allow(clippy::too_many_arguments)]
fn build_scan_config(
    e01_path: &Path,
    volume_offset: u64,
    mft_cluster: u64,
    cluster_size: u64,
    record_size: u32,
    bytes_per_sector: u16,
    mft_data_size: u64,
) -> DbResult<ScanConfig> {
    let scanner = MftScanner::new(
        volume_offset,
        mft_cluster,
        cluster_size,
        record_size,
        bytes_per_sector,
        mft_data_size,
    );
    let data_runs = read_ntfs_mft_data_runs(
        e01_path,
        volume_offset,
        mft_cluster,
        cluster_size,
        record_size,
        bytes_per_sector,
    )
    .map_err(|error| DbError::System(format!("Failed to inspect NTFS $MFT runs: {error}")))?;
    if data_runs.len() > 1 {
        tracing::info!(
            "MFT reader: stitching fragmented $MFT from {} data runs",
            data_runs.len()
        );
    }
    Ok(ScanConfig {
        e01_path: e01_path.to_path_buf(),
        volume_offset,
        mft_cluster,
        cluster_size,
        bytes_per_sector,
        mft_data_size,
        total_records: scanner.total_records(),
        scanner_record_size: scanner.record_size(),
        data_runs,
    })
}

fn run_scan(
    conn: &Connection,
    data_source_id: &DataSourceId,
    config: &ScanConfig,
    progress_fn: ProgressCallback<'_>,
    cancel: Option<Arc<AtomicBool>>,
) -> DbResult<ScanOutput> {
    let (chunk_tx, chunk_rx) = bounded(MFT_CHANNEL_BOUND);
    let (entry_tx, entry_rx) = bounded(MFT_CHANNEL_BOUND);
    let processed = Arc::new(AtomicU64::new(0));
    let insert_errors = Arc::new(AtomicU64::new(0));
    let reader = spawn_reader(config.clone(), chunk_tx, processed.clone(), cancel)?;
    let parsers = spawn_parsers(config, data_source_id, chunk_rx, entry_tx)?;
    let mut output = collect_entries(
        conn,
        entry_rx,
        &processed,
        &insert_errors,
        config.total_records,
        progress_fn,
    );
    join_workers(reader, parsers, &mut output.warnings);
    Ok(output)
}

fn spawn_reader(
    config: ScanConfig,
    chunk_tx: Sender<MftChunk>,
    processed: Arc<AtomicU64>,
    cancel: Option<Arc<AtomicBool>>,
) -> DbResult<JoinHandle<()>> {
    std::thread::Builder::new()
        .name("mft-reader".into())
        .spawn(move || read_chunks(config, chunk_tx, processed, cancel))
        .map_err(|error| DbError::System(format!("Failed to spawn MFT reader: {error}")))
}

fn read_chunks(
    config: ScanConfig,
    chunk_tx: Sender<MftChunk>,
    processed: Arc<AtomicU64>,
    cancel: Option<Arc<AtomicBool>>,
) {
    let mut reader = match E01Reader::open(&config.e01_path) {
        Ok(reader) => reader,
        Err(error) => {
            tracing::error!("MFT reader: failed to open E01: {}", error);
            return;
        }
    };
    let mut start_record = 0u64;
    while start_record < config.total_records {
        if cancel
            .as_ref()
            .is_some_and(|flag| flag.load(Ordering::Relaxed))
        {
            tracing::info!("MFT reader: cancelled");
            return;
        }
        let chunk_count = MFT_CHUNK_RECORDS.min(config.total_records - start_record);
        let mut data = vec![0u8; (chunk_count * config.scanner_record_size as u64) as usize];
        if let Err(error) = read_chunk(&mut reader, &config, start_record, &mut data) {
            tracing::warn!(
                "MFT reader: read error at record {}: {}",
                start_record,
                error
            );
            break;
        }
        if chunk_tx
            .send(MftChunk {
                data,
                start_record,
                count: chunk_count,
            })
            .is_err()
        {
            break;
        }
        start_record += chunk_count;
        processed.store(start_record, Ordering::Relaxed);
    }
}

fn read_chunk(
    reader: &mut E01Reader,
    config: &ScanConfig,
    start_record: u64,
    data: &mut [u8],
) -> std::io::Result<()> {
    let stream_offset = start_record * config.scanner_record_size as u64;
    if config.data_runs.is_empty() {
        read_contiguous_ntfs_mft_stream(
            reader,
            config.volume_offset,
            config.mft_cluster,
            config.cluster_size,
            stream_offset,
            data,
        )
    } else {
        read_ntfs_mft_stream(
            reader,
            config.volume_offset,
            config.cluster_size,
            &config.data_runs,
            stream_offset,
            data,
        )
    }
}

fn spawn_parsers(
    config: &ScanConfig,
    data_source_id: &DataSourceId,
    chunk_rx: Receiver<MftChunk>,
    entry_tx: Sender<Vec<FileEntry>>,
) -> DbResult<Vec<JoinHandle<()>>> {
    let mut handles = Vec::with_capacity(num_cpus::get().clamp(2, 8));
    for parser_id in 0..num_cpus::get().clamp(2, 8) {
        let config = config.clone();
        let receiver = chunk_rx.clone();
        let sender = entry_tx.clone();
        let data_source_id = data_source_id.clone();
        handles.push(
            std::thread::Builder::new()
                .name(format!("mft-parser-{parser_id}"))
                .spawn(move || parse_chunks(config, data_source_id, receiver, sender))
                .map_err(|error| DbError::System(format!("Failed to spawn MFT parser: {error}")))?,
        );
    }
    drop(chunk_rx);
    drop(entry_tx);
    Ok(handles)
}

fn parse_chunks(
    config: ScanConfig,
    data_source_id: DataSourceId,
    receiver: Receiver<MftChunk>,
    sender: Sender<Vec<FileEntry>>,
) {
    let scanner = MftScanner::new(
        config.volume_offset,
        config.mft_cluster,
        config.cluster_size,
        config.scanner_record_size,
        config.bytes_per_sector,
        config.mft_data_size,
    );
    for chunk in receiver.iter() {
        let records = scanner.parse_chunk(&chunk.data, chunk.start_record, chunk.count);
        let entries = records_to_file_entries(&records, &data_source_id);
        if !entries.is_empty() && sender.send(entries).is_err() {
            break;
        }
    }
}

fn collect_entries(
    conn: &Connection,
    receiver: Receiver<Vec<FileEntry>>,
    processed: &AtomicU64,
    insert_errors: &AtomicU64,
    total_records: u64,
    progress_fn: ProgressCallback<'_>,
) -> ScanOutput {
    let repo = FileRepo::new(conn);
    let mut output = empty_output();
    let mut batch = Vec::with_capacity(MFT_DB_BATCH_SIZE);
    for entries in receiver.iter() {
        collect_batch(&mut output, &mut batch, entries);
        flush_full_batch(&repo, &mut batch, &mut output.warnings, insert_errors);
        report_progress(processed, total_records, progress_fn);
    }
    flush_remaining_batch(&repo, &batch, &mut output.warnings);
    output
}

fn collect_batch(output: &mut ScanOutput, batch: &mut Vec<FileEntry>, entries: Vec<FileEntry>) {
    for mut entry in entries {
        match entry.entry_type {
            EntryType::File => {
                output.file_count += 1;
                output.total_size += entry.size.unwrap_or(0);
            }
            EntryType::Directory => output.dir_count += 1,
        }
        add_entry_to_path_map(&mut output.path_map, &mut output.deleted_records, &entry);
        entry.parent_id = None;
        batch.push(entry);
    }
}

fn flush_full_batch(
    repo: &FileRepo<'_>,
    batch: &mut Vec<FileEntry>,
    warnings: &mut Vec<String>,
    insert_errors: &AtomicU64,
) {
    if batch.len() < MFT_DB_BATCH_SIZE {
        return;
    }
    if let Err(error) = repo.insert_batch(batch) {
        warnings.push(format!("DB insert error: {error}"));
        insert_errors.fetch_add(1, Ordering::Relaxed);
    }
    batch.clear();
}

fn flush_remaining_batch(repo: &FileRepo<'_>, batch: &[FileEntry], warnings: &mut Vec<String>) {
    if !batch.is_empty() {
        if let Err(error) = repo.insert_batch(batch) {
            warnings.push(format!("DB insert error: {error}"));
        }
    }
}

fn report_progress(processed: &AtomicU64, total_records: u64, progress_fn: ProgressCallback<'_>) {
    if let Some(progress) = progress_fn {
        let done = processed.load(Ordering::Relaxed);
        let percentage = ((done as f64 / total_records as f64) * 90.0) as u32;
        progress(
            5 + percentage,
            &format!("Scanned {done} / {total_records} MFT records"),
        );
    }
}

fn join_workers(reader: JoinHandle<()>, parsers: Vec<JoinHandle<()>>, warnings: &mut Vec<String>) {
    if let Err(error) = reader.join() {
        warnings.push(format!("MFT reader thread panicked: {error:?}"));
        tracing::error!("MFT reader thread panicked: {:?}", error);
    }
    for parser in parsers {
        if let Err(error) = parser.join() {
            warnings.push(format!("MFT parser thread panicked: {error:?}"));
            tracing::error!("MFT parser thread panicked: {:?}", error);
        }
    }
}

fn finalize_scan(
    conn: &Connection,
    data_source_id: &DataSourceId,
    path_map: &HashMap<String, (Option<String>, String, bool)>,
    deleted_records: &HashSet<String>,
    partition_index: usize,
    progress_fn: ProgressCallback<'_>,
) -> DbResult<()> {
    if let Some(progress) = progress_fn {
        progress(95, "Reconstructing paths...");
    }
    update_entry_paths(
        conn,
        data_source_id,
        path_map,
        deleted_records,
        partition_index,
    )?;
    update_entry_parent_ids(conn, data_source_id, path_map)?;
    if let Some(progress) = progress_fn {
        progress(100, "MFT scan complete");
    }
    Ok(())
}

fn empty_output() -> ScanOutput {
    ScanOutput {
        file_count: 0,
        dir_count: 0,
        total_size: 0,
        warnings: Vec::new(),
        path_map: HashMap::new(),
        deleted_records: HashSet::new(),
    }
}
