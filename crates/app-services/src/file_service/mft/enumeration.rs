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
use persistence_sqlite::{repositories::file_repo::FileRepo, DbError, DbResult};
use rusqlite::Connection;

use crate::file_service::enumeration::EnumerationStats;

use super::{
    graph::populate_file_graph_for_data_source,
    reader::{self, MftChunk, MftReaderConfig},
    records::{
        add_entry_to_path_map, records_to_file_entries_with_partition,
        update_entry_parent_ids_in_transaction, update_entry_paths_in_transaction,
    },
    stream::read_ntfs_mft_data_runs,
    workers,
};

const MFT_CHANNEL_BOUND: usize = 4;
const MFT_DB_BATCH_SIZE: usize = 2_000;
type ProgressCallback<'a> = Option<&'a dyn Fn(u32, &str)>;

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
    partition_index: Option<usize>,
}

#[derive(Default)]
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
    enumerate_filesystem_mft_inner(
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
        None,
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
    enumerate_filesystem_mft_inner(
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
        Some(partition_index),
    )
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn enumerate_filesystem_mft_inner(
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
    partition_index: Option<usize>,
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
        partition_index,
    )?;
    let transaction = conn.unchecked_transaction()?;
    let mut output = run_scan(&transaction, data_source_id, &config, progress_fn, cancel)?;
    finalize_scan(
        &transaction,
        data_source_id,
        &output.path_map,
        &output.deleted_records,
        partition_index,
        progress_fn,
    )?;
    transaction.commit()?;
    if let Err(error) = populate_file_graph_for_data_source(conn, data_source_id) {
        output
            .warnings
            .push(format!("Graph population warning: {error}"));
        tracing::warn!(
            "Failed to populate file graph after MFT enumeration: {}",
            error
        );
    }
    if let Some(progress) = progress_fn {
        progress(100, "MFT scan complete");
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
    partition_index: Option<usize>,
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
        partition_index,
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
    let pipeline_stop = Arc::new(AtomicBool::new(false));
    let parsers = spawn_parsers(config, data_source_id, chunk_rx, entry_tx)?;
    let reader = reader::spawn_reader(
        MftReaderConfig {
            e01_path: config.e01_path.clone(),
            volume_offset: config.volume_offset,
            mft_cluster: config.mft_cluster,
            cluster_size: config.cluster_size,
            total_records: config.total_records,
            scanner_record_size: config.scanner_record_size,
            data_runs: config.data_runs.clone(),
        },
        chunk_tx,
        processed.clone(),
        cancel,
        pipeline_stop.clone(),
    )?;
    let collected = collect_entries(
        conn,
        entry_rx,
        &processed,
        &pipeline_stop,
        config.total_records,
        progress_fn,
    );
    let mut output = match collected {
        Ok(output) => output,
        Err(persistence_error) => {
            let mut worker_warnings = Vec::new();
            if let Err(worker_error) = workers::join_workers(reader, parsers, &mut worker_warnings)
            {
                tracing::debug!(
                    error = %worker_error,
                    "MFT workers stopped after a persistence failure"
                );
            }
            return Err(persistence_error);
        }
    };
    workers::join_workers(reader, parsers, &mut output.warnings)?;
    let processed_records = processed.load(Ordering::Relaxed);
    if processed_records != config.total_records {
        return Err(DbError::System(format!(
            "MFT enumeration stopped after {processed_records} of {} records",
            config.total_records
        )));
    }
    Ok(output)
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
        let entries = records_to_file_entries_with_partition(
            &records,
            &data_source_id,
            config.partition_index,
        );
        if !entries.is_empty() && sender.send(entries).is_err() {
            break;
        }
    }
}

fn collect_entries(
    conn: &Connection,
    receiver: Receiver<Vec<FileEntry>>,
    processed: &AtomicU64,
    pipeline_stop: &AtomicBool,
    total_records: u64,
    progress_fn: ProgressCallback<'_>,
) -> DbResult<ScanOutput> {
    let repo = FileRepo::new(conn);
    let mut output = ScanOutput::default();
    let mut batch = Vec::with_capacity(MFT_DB_BATCH_SIZE);
    for entries in receiver.iter() {
        collect_batch(&mut output, &mut batch, entries);
        if let Err(error) = flush_full_batch(&repo, &mut batch) {
            pipeline_stop.store(true, Ordering::Relaxed);
            return Err(error);
        }
        report_progress(processed, total_records, progress_fn);
    }
    if let Err(error) = flush_remaining_batch(&repo, &batch) {
        pipeline_stop.store(true, Ordering::Relaxed);
        return Err(error);
    }
    Ok(output)
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

fn flush_full_batch(repo: &FileRepo<'_>, batch: &mut Vec<FileEntry>) -> DbResult<()> {
    if batch.len() < MFT_DB_BATCH_SIZE {
        return Ok(());
    }
    repo.insert_batch_unchecked(batch)?;
    batch.clear();
    Ok(())
}

fn flush_remaining_batch(repo: &FileRepo<'_>, batch: &[FileEntry]) -> DbResult<()> {
    if !batch.is_empty() {
        repo.insert_batch_unchecked(batch)?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "../../../tests/unit/file_service/mft/enumeration.rs"]
mod tests;

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

fn finalize_scan(
    conn: &Connection,
    data_source_id: &DataSourceId,
    path_map: &HashMap<String, (Option<String>, String, bool)>,
    deleted_records: &HashSet<String>,
    partition_index: Option<usize>,
    progress_fn: ProgressCallback<'_>,
) -> DbResult<()> {
    if let Some(progress) = progress_fn {
        progress(95, "Reconstructing paths...");
    }
    update_entry_paths_in_transaction(
        conn,
        data_source_id,
        path_map,
        deleted_records,
        partition_index.unwrap_or(0),
        partition_index,
    )?;
    update_entry_parent_ids_in_transaction(conn, data_source_id, path_map, partition_index)?;
    Ok(())
}
