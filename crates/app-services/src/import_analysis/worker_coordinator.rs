use super::error::ImportAnalysisError;
use super::options::{AnalysisProgressCallback, ImportAnalysisOptions, ImportAnalysisStats};
use super::progress::{current_rss_mb, memory_hard_limit_exceeded, rows_per_sec, scheduling_state};
use super::task_feed::{analysis_task_queue_bound, enqueue_analysis_tasks_prioritized};
use super::worker_model::WorkerStats;
use super::worker_runtime::{add_worker_stats, run_analysis_worker, FileTask, SharedAnalysisState};
use crossbeam_channel::{bounded, Receiver};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

pub(super) fn run_analysis_workers(
    options: &ImportAnalysisOptions,
    worker_ids: &[usize],
    estimated: u64,
    memory_soft_limit_mb: u64,
    memory_hard_limit_mb: u64,
    analysis_started: Instant,
    progress_cb: Option<AnalysisProgressCallback<'_>>,
) -> Result<ImportAnalysisStats, ImportAnalysisError> {
    let group = spawn_worker_group(options, worker_ids)?;
    let mut stats = ImportAnalysisStats {
        worker_ids: worker_ids.to_vec(),
        ..ImportAnalysisStats::default()
    };
    let context = WorkerRunContext {
        options,
        estimated,
        memory_soft_limit_mb,
        memory_hard_limit_mb,
        analysis_started,
        progress_cb,
    };
    collect_worker_results(&group, &mut stats, &context);
    finish_worker_group(group, &mut stats)?;
    Ok(stats)
}

struct WorkerRunContext<'options, 'progress> {
    options: &'options ImportAnalysisOptions,
    estimated: u64,
    memory_soft_limit_mb: u64,
    memory_hard_limit_mb: u64,
    analysis_started: Instant,
    progress_cb: Option<AnalysisProgressCallback<'progress>>,
}

struct AnalysisWorkerGroup {
    producer: JoinHandle<Result<(), ImportAnalysisError>>,
    workers: Vec<JoinHandle<()>>,
    result_rx: Receiver<(usize, Result<WorkerStats, ImportAnalysisError>)>,
    shared: Arc<SharedAnalysisState>,
}

fn spawn_worker_group(
    options: &ImportAnalysisOptions,
    worker_ids: &[usize],
) -> Result<AnalysisWorkerGroup, ImportAnalysisError> {
    let worker_count = worker_ids.len();
    let (task_tx, task_rx) = bounded(analysis_task_queue_bound(worker_count));
    let (result_tx, result_rx) = bounded(worker_count);
    let shared = Arc::new(SharedAnalysisState::new());
    let producer_options = options.clone();
    let producer_shared = Arc::clone(&shared);
    let producer = std::thread::Builder::new()
        .name("analysis-task-producer".to_string())
        .spawn(move || {
            let result =
                enqueue_analysis_tasks_prioritized(&producer_options, &task_tx, producer_shared);
            drop(task_tx);
            result
        })
        .map_err(|error| ImportAnalysisError::Other(format!("Spawn analysis producer: {error}")))?;

    let mut workers = Vec::with_capacity(worker_count);
    for worker_id in worker_ids.iter().copied() {
        workers.push(spawn_worker(
            worker_id, options, &task_rx, &result_tx, &shared,
        )?);
    }
    drop(task_rx);
    drop(result_tx);
    Ok(AnalysisWorkerGroup {
        producer,
        workers,
        result_rx,
        shared,
    })
}

fn spawn_worker(
    worker_id: usize,
    options: &ImportAnalysisOptions,
    task_rx: &Receiver<FileTask>,
    result_tx: &crossbeam_channel::Sender<(usize, Result<WorkerStats, ImportAnalysisError>)>,
    shared: &Arc<SharedAnalysisState>,
) -> Result<JoinHandle<()>, ImportAnalysisError> {
    let rx = task_rx.clone();
    let tx = result_tx.clone();
    let worker_options = options.clone();
    let shared = Arc::clone(shared);
    std::thread::Builder::new()
        .name(format!("analysis-worker-{worker_id}"))
        .spawn(move || {
            shared.active_workers.fetch_add(1, Ordering::Relaxed);
            let result = run_analysis_worker(worker_id, worker_options, rx, Arc::clone(&shared));
            shared.active_workers.fetch_sub(1, Ordering::Relaxed);
            let _ = tx.send((worker_id, result));
        })
        .map_err(|error| {
            ImportAnalysisError::Other(format!("Spawn analysis worker {worker_id}: {error}"))
        })
}

fn collect_worker_results(
    group: &AnalysisWorkerGroup,
    stats: &mut ImportAnalysisStats,
    context: &WorkerRunContext<'_, '_>,
) {
    let worker_count = stats.worker_ids.len();
    let mut completed = 0usize;
    while completed < worker_count {
        match group.result_rx.recv_timeout(Duration::from_millis(250)) {
            Ok((_worker_id, worker_result)) => {
                completed += 1;
                record_worker_result(stats, worker_result);
                emit_worker_completion(context, &group.shared, completed, worker_count);
            }
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                emit_worker_heartbeat(context, &group.shared, worker_count)
            }
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
        }
    }
}

fn record_worker_result(
    stats: &mut ImportAnalysisStats,
    worker_result: Result<WorkerStats, ImportAnalysisError>,
) {
    match worker_result {
        Ok(worker_stats) => add_worker_stats(stats, worker_stats),
        Err(error) => {
            stats.warning_count = stats.warning_count.saturating_add(1);
            stats.failed_count = stats.failed_count.saturating_add(1);
            tracing::warn!("Analysis worker failed: {}", error);
        }
    }
}

fn emit_worker_completion(
    context: &WorkerRunContext<'_, '_>,
    shared: &SharedAnalysisState,
    completed: usize,
    worker_count: usize,
) {
    let Some(cb) = context.progress_cb else {
        return;
    };
    let pct = 72 + ((completed as u32 * 10) / worker_count as u32);
    cb(
        pct.min(82),
        &format!(
            "Analysis workers done: phase=analysis scheduling={} workersDone={}/{} workerBudget={} processed={} rowsPerSec={} queuedTasks={} pendingTasks={} indexed={} activeWorkers={} rssMb={}",
            if context.options.cancel_token.load(Ordering::Relaxed) {
                "draining"
            } else {
                "running"
            },
            completed,
            worker_count,
            worker_count,
            shared.processed_total.load(Ordering::Relaxed),
            rows_per_sec(
                shared.processed_total.load(Ordering::Relaxed) as u64,
                context.analysis_started.elapsed()
            ),
            shared.queued_total.load(Ordering::Relaxed),
            context
                .estimated
                .saturating_sub(shared.processed_total.load(Ordering::Relaxed) as u64),
            shared.indexed_total.load(Ordering::Relaxed),
            shared.active_workers.load(Ordering::Relaxed),
            current_rss_mb()
        ),
    );
}

fn emit_worker_heartbeat(
    context: &WorkerRunContext<'_, '_>,
    shared: &SharedAnalysisState,
    worker_count: usize,
) {
    let rss_mb = current_rss_mb();
    if memory_hard_limit_exceeded(context.memory_hard_limit_mb) {
        context.options.cancel_token.store(true, Ordering::Relaxed);
        if let Some(cb) = context.progress_cb {
            cb(
                75,
                &format!(
                    "Analysis memory hard limit exceeded: phase=analysis scheduling=draining rssMb={} hardLimitMb={} workerBudget={} queuedTasks={} pendingTasks={} processed={} activeWorkers={}",
                    rss_mb,
                    context.memory_hard_limit_mb,
                    worker_count,
                    shared.queued_total.load(Ordering::Relaxed),
                    context.estimated.saturating_sub(
                        shared.processed_total.load(Ordering::Relaxed) as u64
                    ),
                    shared.processed_total.load(Ordering::Relaxed),
                    shared.active_workers.load(Ordering::Relaxed)
                ),
            );
        }
    } else if let Some(cb) = context.progress_cb {
        let level = if rss_mb >= context.memory_soft_limit_mb {
            "soft-limit"
        } else {
            "ok"
        };
        cb(
            75,
            &format!(
                "Analysis heartbeat: phase=analysis scheduling={} memory={} rssMb={} softLimitMb={} hardLimitMb={} workerBudget={} queuedTasks={} pendingTasks={} processed={}/{} indexed={} activeWorkers={}",
                scheduling_state(
                    context.options.cancel_token.load(Ordering::Relaxed),
                    rss_mb,
                    context.memory_soft_limit_mb
                ),
                level,
                rss_mb,
                context.memory_soft_limit_mb,
                context.memory_hard_limit_mb,
                worker_count,
                shared.queued_total.load(Ordering::Relaxed),
                context.estimated.saturating_sub(
                    shared.processed_total.load(Ordering::Relaxed) as u64
                ),
                shared.processed_total.load(Ordering::Relaxed),
                context.estimated,
                shared.indexed_total.load(Ordering::Relaxed),
                shared.active_workers.load(Ordering::Relaxed)
            ),
        );
    }
}

fn finish_worker_group(
    group: AnalysisWorkerGroup,
    stats: &mut ImportAnalysisStats,
) -> Result<(), ImportAnalysisError> {
    let producer_result = group
        .producer
        .join()
        .map_err(|_| ImportAnalysisError::Other("Analysis task producer panicked".to_string()))?;
    if let Err(error) = producer_result {
        stats.warning_count = stats.warning_count.saturating_add(1);
        stats.failed_count = stats.failed_count.saturating_add(1);
        return Err(error);
    }
    for handle in group.workers {
        handle
            .join()
            .map_err(|_| ImportAnalysisError::Other("Analysis worker panicked".to_string()))?;
    }
    Ok(())
}
