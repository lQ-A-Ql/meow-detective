use super::partition_work::{enumerate_single_partition, PartitionResult, PartitionWork};
use super::progress::{heartbeat_percent, PROGRESS_CHANNEL_CAPACITY};
use crossbeam_channel::{bounded, Receiver, Sender};
use domain::DataSourceId;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

type ProgressMessage = (usize, u64, u64);

struct Channels {
    work_tx: Sender<PartitionWork>,
    work_rx: Receiver<PartitionWork>,
    result_tx: Sender<PartitionResult>,
    result_rx: Receiver<PartitionResult>,
    progress_tx: Sender<ProgressMessage>,
    progress_rx: Receiver<ProgressMessage>,
}

impl Channels {
    fn new(work_capacity: usize, worker_count: usize) -> Self {
        let (work_tx, work_rx) = bounded(work_capacity.max(1));
        let (result_tx, result_rx) = bounded(worker_count.max(1));
        let (progress_tx, progress_rx) = bounded(PROGRESS_CHANNEL_CAPACITY);
        Self {
            work_tx,
            work_rx,
            result_tx,
            result_rx,
            progress_tx,
            progress_rx,
        }
    }
}

/// Enumerate partitions into independent staging databases.
pub fn enumerate_partitions_parallel(
    case_root: &Path,
    data_source_id: &DataSourceId,
    partitions: Vec<PartitionWork>,
    max_workers: usize,
    cancel_token: Arc<AtomicBool>,
    progress_cb: &dyn Fn(usize, u32, &str),
) -> Result<Vec<PartitionResult>, String> {
    if partitions.is_empty() {
        return Ok(Vec::new());
    }

    let worker_count = effective_worker_count(&partitions, max_workers);
    let channels = Channels::new(partitions.len(), worker_count);
    let submitted = submit_work(partitions, &channels.work_tx, &cancel_token, progress_cb)?;
    if submitted == 0 {
        return Ok(Vec::new());
    }

    let handles = spawn_workers(
        worker_count.min(submitted),
        case_root,
        data_source_id,
        &channels,
        cancel_token,
    )?;
    drop(channels.work_tx);
    drop(channels.result_tx);
    drop(channels.progress_tx);

    let mut results = collect_results(
        submitted,
        &channels.result_rx,
        &channels.progress_rx,
        progress_cb,
    );
    join_workers(handles)?;
    results.sort_by_key(|result| result.index);
    Ok(results)
}

/// Resolve the actual partition-worker count after evidence-reader safeguards.
pub fn effective_worker_count(partitions: &[PartitionWork], requested: usize) -> usize {
    let bounded = partitions.len().min(requested.max(1)).max(1);
    if partitions.iter().any(PartitionWork::uses_e01_reader) {
        1
    } else {
        bounded
    }
}

fn submit_work(
    partitions: Vec<PartitionWork>,
    sender: &Sender<PartitionWork>,
    cancel_token: &AtomicBool,
    progress_cb: &dyn Fn(usize, u32, &str),
) -> Result<usize, String> {
    let mut submitted = 0;
    for partition in partitions {
        if cancel_token.load(Ordering::Relaxed) {
            break;
        }
        progress_cb(partition.index, 0, &format!("Starting {}", partition.name));
        sender
            .send(partition)
            .map_err(|error| format!("Failed to queue partition work: {error}"))?;
        submitted += 1;
    }
    Ok(submitted)
}

fn spawn_workers(
    worker_count: usize,
    case_root: &Path,
    data_source_id: &DataSourceId,
    channels: &Channels,
    cancel_token: Arc<AtomicBool>,
) -> Result<Vec<JoinHandle<()>>, String> {
    let mut handles = Vec::with_capacity(worker_count);
    for worker_index in 0..worker_count {
        handles.push(spawn_worker(
            worker_index,
            case_root.to_path_buf(),
            data_source_id.0.clone(),
            channels.work_rx.clone(),
            channels.result_tx.clone(),
            channels.progress_tx.clone(),
            cancel_token.clone(),
        )?);
    }
    Ok(handles)
}

fn spawn_worker(
    worker_index: usize,
    case_root: PathBuf,
    data_source_id: String,
    work_rx: Receiver<PartitionWork>,
    result_tx: Sender<PartitionResult>,
    progress_tx: Sender<ProgressMessage>,
    cancel_token: Arc<AtomicBool>,
) -> Result<JoinHandle<()>, String> {
    std::thread::Builder::new()
        .name(format!("enum-worker-{worker_index}"))
        .spawn(move || {
            while let Ok(partition) = work_rx.recv() {
                let index = partition.index;
                if cancel_token.load(Ordering::Relaxed) {
                    let _ = result_tx.send(PartitionResult::cancelled(index));
                    break;
                }
                let progress = |entries, size| {
                    let _ = progress_tx.try_send((index, entries, size));
                };
                let result = enumerate_single_partition(
                    &case_root,
                    &data_source_id,
                    partition,
                    &cancel_token,
                    Some(&progress),
                );
                let _ = result_tx.send(result);
                if cancel_token.load(Ordering::Relaxed) {
                    break;
                }
            }
        })
        .map_err(|error| format!("Failed to spawn thread: {error}"))
}

fn collect_results(
    submitted: usize,
    result_rx: &Receiver<PartitionResult>,
    progress_rx: &Receiver<ProgressMessage>,
    progress_cb: &dyn Fn(usize, u32, &str),
) -> Vec<PartitionResult> {
    let mut results = Vec::with_capacity(submitted);
    while results.len() < submitted {
        drain_progress(progress_rx, results.len(), submitted, progress_cb);
        match result_rx.recv_timeout(Duration::from_millis(25)) {
            Ok(result) => {
                report_completion(&result, progress_cb);
                results.push(result);
            }
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {}
        }
    }
    drain_progress(progress_rx, results.len(), submitted, progress_cb);
    results
}

fn drain_progress(
    progress_rx: &Receiver<ProgressMessage>,
    done_count: usize,
    submitted: usize,
    progress_cb: &dyn Fn(usize, u32, &str),
) {
    while let Ok((index, entries, _)) = progress_rx.try_recv() {
        let percent = heartbeat_percent(done_count, submitted, entries);
        progress_cb(
            index,
            percent,
            &format!("Partition {index}: {entries} entries"),
        );
    }
}

fn report_completion(result: &PartitionResult, progress_cb: &dyn Fn(usize, u32, &str)) {
    let state = if result.error.is_some() {
        "failed"
    } else {
        "done"
    };
    progress_cb(
        result.index,
        100,
        &format!("Partition {} {state}", result.index),
    );
}

fn join_workers(handles: Vec<JoinHandle<()>>) -> Result<(), String> {
    let mut panicked = false;
    for handle in handles {
        if let Err(error) = handle.join() {
            tracing::error!("Enumeration thread panicked: {:?}", error);
            panicked = true;
        }
    }
    if panicked {
        Err("Enumeration worker panicked".to_string())
    } else {
        Ok(())
    }
}

/// Get the opt-in upper bound for explicit worker settings.
pub fn default_worker_count() -> usize {
    crate::import_scheduler::default_cpu_budget()
}

/// Resolve worker count from settings.
pub fn resolve_worker_count(max_import_workers: Option<usize>) -> usize {
    crate::import_scheduler::resolve_import_worker_count(max_import_workers)
}
