use super::partition_worker::{enumerate_single_partition, PartitionResult, PartitionWork};
use super::progress::{heartbeat_percent, PROGRESS_CHANNEL_CAPACITY};
use crossbeam_channel::{bounded, Receiver, Sender};
use domain::DataSourceId;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

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
