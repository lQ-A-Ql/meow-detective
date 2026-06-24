use super::budget::{
    content_budget_for_mode, default_memory_hard_limit_mb, default_memory_soft_limit_mb,
};
use super::error::ImportAnalysisError;
use super::finalize::{
    collect_done_worker_stats, discover_analysis_worker_ids, merge_finished_analysis_staging,
    prepare_analysis_staging_startup, AnalysisStartupAction,
};
use super::options::{
    AnalysisProgressCallback, ImportAnalysisOptions, ImportAnalysisStats, JobOutcomeCounts,
    PostImportPipelineError, PostImportPipelineOptions,
};
use super::progress::{
    bool_word, current_rss_mb, memory_hard_limit_exceeded, rows_per_sec, scheduling_state,
};
use super::task_feed::{
    analysis_task_queue_bound, count_analysis_file_tasks, enqueue_analysis_tasks_prioritized,
};
use super::tier::advance_tier;
use super::worker_runtime::{add_worker_stats, run_analysis_worker, FileTask, SharedAnalysisState};
use crossbeam_channel::{bounded, Receiver, Sender};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};

pub fn run_post_import_pipeline_with_counts(
    options: PostImportPipelineOptions,
    progress_cb: Option<AnalysisProgressCallback<'_>>,
) -> Result<(String, JobOutcomeCounts), PostImportPipelineError> {
    let mut counts = JobOutcomeCounts::default();
    // Clone before options is partially moved constructing ImportAnalysisOptions.
    let tier_state = Arc::clone(&options.tier_state);

    if !options.enable_timeline_projection
        && !options.enable_content_extraction
        && !options.enable_text_indexing
    {
        if let Some(cb) = progress_cb {
            cb(
                84,
                &format!(
                    "Post-import skipped: phase=post-import-skip scheduling=deferred workerBudget={} activeWorkers=0 queuedTasks=0 pendingTasks=0 timeline=deferred content=disabled text=disabled contentDeferred=true textDeferred=true",
                    resolve_analysis_worker_count(options.max_analysis_workers).max(1)
                ),
            );
        }
        // All tiers skipped — mark them done for consistent inspection.
        {
            let mut ts = tier_state.lock().map_err(|_| PostImportPipelineError {
                message: "tier state lock poisoned".to_string(),
                counts: counts.clone(),
            })?;
            while advance_tier(&mut ts).is_some() {}
        }
        return Ok((
            "Timeline: deferred until Timeline page. Artifacts: 0. Index: 0 indexed".to_string(),
            counts,
        ));
    }

    let stats = match run_import_analysis_staging(
        {
            let content_deferred =
                !options.enable_content_extraction || !options.analysis_mode.allows_content();
            let text_deferred =
                !options.enable_text_indexing || !options.analysis_mode.allows_content();
            if let Some(cb) = progress_cb {
                cb(
                    70,
                    &format!(
                        "Post-import analysis scheduled: phase=analysis-start scheduling=queued mode={} workerBudget={} activeWorkers=0 queuedTasks=0 pendingTasks=unknown contentDeferred={} textDeferred={}",
                        options.analysis_mode.as_str(),
                        resolve_analysis_worker_count(options.max_analysis_workers).max(1),
                        bool_word(content_deferred),
                        bool_word(text_deferred)
                    ),
                );
            }
            ImportAnalysisOptions {
                case_root: options.case_root,
                db_path: options.db_path,
                case_id: options.case_id,
                data_source_id: options.data_source_id,
                index_dir: options.index_dir,
                max_analysis_workers: options.max_analysis_workers,
                cancel_token: options.cancel_token,
                enable_timeline_projection: options.enable_timeline_projection,
                enable_content_extraction: options.enable_content_extraction,
                enable_text_indexing: options.enable_text_indexing,
                analysis_mode: options.analysis_mode,
                content_budget: content_budget_for_mode(options.analysis_mode),
                memory_soft_limit_mb: default_memory_soft_limit_mb(),
                memory_hard_limit_mb: default_memory_hard_limit_mb(),
                tier_state: Arc::clone(&tier_state),
            }
        },
        progress_cb,
    ) {
        Ok(stats) => stats,
        Err(error) => {
            counts.add_warnings(1);
            let message = error.to_string();
            if message.to_ascii_lowercase().contains("cancel") {
                counts.add_skipped(1);
            } else {
                counts.add_failed(1);
            }
            return Err(PostImportPipelineError { message, counts });
        }
    };

    // All tiers complete — advance to None (marks CorrelateAndIndex as done).
    {
        let mut ts = tier_state.lock().map_err(|_| PostImportPipelineError {
            message: "tier state lock poisoned".to_string(),
            counts: counts.clone(),
        })?;
        while advance_tier(&mut ts).is_some() {}
    }

    counts.warning_count = counts.warning_count.saturating_add(stats.warning_count);
    counts.skipped_count = counts.skipped_count.saturating_add(stats.skipped_count);
    counts.failed_count = counts.failed_count.saturating_add(stats.failed_count);

    let mut message = format!(
        "Timeline: {} events. Artifacts: {}. Index: {} indexed",
        stats.timeline_count, stats.artifact_count, stats.indexed_count
    );
    if counts.is_partial() {
        message.push_str(&format!(
            ". Partial: {} warnings, {} skipped, {} failed",
            counts.warning_count, counts.skipped_count, counts.failed_count
        ));
    }

    Ok((message, counts))
}

pub fn default_analysis_worker_count() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get().min(4))
        .unwrap_or(4)
}

pub fn resolve_analysis_worker_count(max_analysis_workers: Option<usize>) -> usize {
    match max_analysis_workers {
        Some(n) if n > 0 => n.min(default_analysis_worker_count()),
        _ => default_analysis_worker_count(),
    }
}

pub fn run_import_analysis_staging(
    options: ImportAnalysisOptions,
    progress_cb: Option<AnalysisProgressCallback<'_>>,
) -> Result<ImportAnalysisStats, ImportAnalysisError> {
    let analysis_started = Instant::now();
    let worker_count = resolve_analysis_worker_count(options.max_analysis_workers).max(1);
    let worker_ids: Vec<usize> = (0..worker_count).collect();
    let memory_soft_limit_mb = if options.memory_soft_limit_mb == 0 {
        default_memory_soft_limit_mb()
    } else {
        options.memory_soft_limit_mb
    };
    let memory_hard_limit_mb = if options.memory_hard_limit_mb == 0 {
        default_memory_hard_limit_mb()
    } else {
        options.memory_hard_limit_mb.max(memory_soft_limit_mb + 1)
    };

    let startup_action =
        prepare_analysis_staging_startup(&options, &worker_ids, worker_count, progress_cb)?;

    if startup_action == AnalysisStartupAction::AlreadyMerged {
        if let Some(cb) = progress_cb {
            cb(
                94,
                "Analysis staging already merged; skipping analysis resume.",
            );
        }
        // All tiers were completed in a previous run.
        {
            let mut ts = options
                .tier_state
                .lock()
                .map_err(|_| ImportAnalysisError::Other("tier state lock poisoned".to_string()))?;
            while advance_tier(&mut ts).is_some() {}
        }
        let merged_worker_ids =
            discover_analysis_worker_ids(&options.case_root, &options.data_source_id.0)?;
        return collect_done_worker_stats(&options, &merged_worker_ids);
    }

    let estimated = count_analysis_file_tasks(&options.db_path, &options.data_source_id)?;
    if memory_hard_limit_exceeded(memory_hard_limit_mb) {
        options.cancel_token.store(true, Ordering::Relaxed);
        return Err(ImportAnalysisError::Other(format!(
            "Import analysis memory hard limit exceeded before start: rssMb={} hardLimitMb={}",
            current_rss_mb(),
            memory_hard_limit_mb
        )));
    }
    if let Some(cb) = progress_cb {
        cb(
            72,
            &format!(
                "Analysis staging: phase=analysis-start scheduling=queued mode={} workers={} workerBudget={} activeWorkers=0 queuedTasks=0 pendingTasks={} queueBound={} content={} text={} contentDeferred={} textDeferred={} rssMb={}",
                options.analysis_mode.as_str(),
                worker_count,
                worker_count,
                estimated,
                analysis_task_queue_bound(worker_count),
                if options.enable_content_extraction {
                    "enabled"
                } else {
                    "disabled"
                },
                if options.enable_text_indexing {
                    "enabled"
                } else {
                    "disabled"
                },
                bool_word(!options.enable_content_extraction || !options.analysis_mode.allows_content()),
                bool_word(!options.enable_text_indexing || !options.analysis_mode.allows_content()),
                current_rss_mb()
            ),
        );
    }

    // Begin Catalog tier (MFT enumeration / file counting).
    {
        let mut ts = options
            .tier_state
            .lock()
            .map_err(|_| ImportAnalysisError::Other("tier state lock poisoned".to_string()))?;
        advance_tier(&mut ts);
    }

    if startup_action == AnalysisStartupAction::MergeOnly {
        // Producer and workers completed in a previous run.
        {
            let mut ts = options
                .tier_state
                .lock()
                .map_err(|_| ImportAnalysisError::Other("tier state lock poisoned".to_string()))?;
            advance_tier(&mut ts); // → Catalog
            advance_tier(&mut ts); // → ExtractArtifacts
        }
        let stats = collect_done_worker_stats(&options, &worker_ids)?;
        let result = merge_finished_analysis_staging(
            &options,
            &worker_ids,
            stats,
            progress_cb,
            analysis_started,
        )?;
        // Merge done — advance to CorrelateAndIndex.
        {
            let mut ts = options
                .tier_state
                .lock()
                .map_err(|_| ImportAnalysisError::Other("tier state lock poisoned".to_string()))?;
            advance_tier(&mut ts);
        }
        return Ok(result);
    }

    let queue_bound = analysis_task_queue_bound(worker_count);
    let (task_tx, task_rx): (Sender<FileTask>, Receiver<FileTask>) = bounded(queue_bound);
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
        .map_err(|e| ImportAnalysisError::Other(format!("Spawn analysis producer: {e}")))?;

    let mut handles = Vec::with_capacity(worker_count);
    for worker_id in worker_ids.iter().copied() {
        let rx = task_rx.clone();
        let tx = result_tx.clone();
        let worker_options = options.clone();
        let shared = Arc::clone(&shared);
        let handle = std::thread::Builder::new()
            .name(format!("analysis-worker-{worker_id}"))
            .spawn(move || {
                shared.active_workers.fetch_add(1, Ordering::Relaxed);
                let result = run_analysis_worker(worker_id, worker_options, rx, shared.clone());
                shared.active_workers.fetch_sub(1, Ordering::Relaxed);
                let _ = tx.send((worker_id, result));
            })
            .map_err(|e| {
                ImportAnalysisError::Other(format!("Spawn analysis worker {worker_id}: {e}"))
            })?;
        handles.push(handle);
    }
    drop(task_rx);
    drop(result_tx);

    let mut completed = 0usize;
    let mut stats = ImportAnalysisStats {
        worker_ids: worker_ids.clone(),
        ..ImportAnalysisStats::default()
    };
    while completed < worker_count {
        match result_rx.recv_timeout(Duration::from_millis(250)) {
            Ok((_worker_id, worker_result)) => {
                completed += 1;
                match worker_result {
                    Ok(worker_stats) => add_worker_stats(&mut stats, worker_stats),
                    Err(error) => {
                        stats.warning_count = stats.warning_count.saturating_add(1);
                        stats.failed_count = stats.failed_count.saturating_add(1);
                        tracing::warn!("Analysis worker failed: {}", error);
                    }
                }
                if let Some(cb) = progress_cb {
                    let pct = 72 + ((completed as u32 * 10) / worker_count as u32);
                    cb(
                        pct.min(82),
                        &format!(
                            "Analysis workers done: phase=analysis scheduling={} workersDone={}/{} workerBudget={} processed={} rowsPerSec={} queuedTasks={} pendingTasks={} indexed={} activeWorkers={} rssMb={}",
                            if options.cancel_token.load(Ordering::Relaxed) {
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
                                analysis_started.elapsed()
                            ),
                            shared.queued_total.load(Ordering::Relaxed),
                            estimated.saturating_sub(
                                shared.processed_total.load(Ordering::Relaxed) as u64
                            ),
                            shared.indexed_total.load(Ordering::Relaxed),
                            shared.active_workers.load(Ordering::Relaxed),
                            current_rss_mb()
                        ),
                    );
                }
            }
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                let rss_mb = current_rss_mb();
                if memory_hard_limit_exceeded(memory_hard_limit_mb) {
                    options.cancel_token.store(true, Ordering::Relaxed);
                    if let Some(cb) = progress_cb {
                        cb(
                            75,
                            &format!(
                                "Analysis memory hard limit exceeded: phase=analysis scheduling=draining rssMb={} hardLimitMb={} workerBudget={} queuedTasks={} pendingTasks={} processed={} activeWorkers={}",
                                rss_mb,
                                memory_hard_limit_mb,
                                worker_count,
                                shared.queued_total.load(Ordering::Relaxed),
                                estimated.saturating_sub(
                                    shared.processed_total.load(Ordering::Relaxed) as u64
                                ),
                                shared.processed_total.load(Ordering::Relaxed),
                                shared.active_workers.load(Ordering::Relaxed)
                            ),
                        );
                    }
                } else if let Some(cb) = progress_cb {
                    let level = if rss_mb >= memory_soft_limit_mb {
                        "soft-limit"
                    } else {
                        "ok"
                    };
                    cb(
                        75,
                        &format!(
                            "Analysis heartbeat: phase=analysis scheduling={} memory={} rssMb={} softLimitMb={} hardLimitMb={} workerBudget={} queuedTasks={} pendingTasks={} processed={}/{} indexed={} activeWorkers={}",
                            scheduling_state(options.cancel_token.load(Ordering::Relaxed), rss_mb, memory_soft_limit_mb),
                            level,
                            rss_mb,
                            memory_soft_limit_mb,
                            memory_hard_limit_mb,
                            worker_count,
                            shared.queued_total.load(Ordering::Relaxed),
                            estimated.saturating_sub(
                                shared.processed_total.load(Ordering::Relaxed) as u64
                            ),
                            shared.processed_total.load(Ordering::Relaxed),
                            estimated,
                            shared.indexed_total.load(Ordering::Relaxed),
                            shared.active_workers.load(Ordering::Relaxed)
                        ),
                    );
                }
            }
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
        }
    }

    let producer_result = producer
        .join()
        .map_err(|_| ImportAnalysisError::Other("Analysis task producer panicked".to_string()))?;
    if let Err(error) = producer_result {
        stats.warning_count = stats.warning_count.saturating_add(1);
        stats.failed_count = stats.failed_count.saturating_add(1);
        return Err(error);
    }

    for handle in handles {
        handle
            .join()
            .map_err(|_| ImportAnalysisError::Other("Analysis worker panicked".to_string()))?;
    }

    // MFT enumeration done — advance to ExtractArtifacts tier.
    {
        let mut ts = options
            .tier_state
            .lock()
            .map_err(|_| ImportAnalysisError::Other("tier state lock poisoned".to_string()))?;
        advance_tier(&mut ts);
    }

    if options.cancel_token.load(Ordering::Relaxed) {
        stats.warning_count = stats.warning_count.saturating_add(1);
        return Err(ImportAnalysisError::Other(
            "Import analysis cancelled by user".to_string(),
        ));
    }

    let result = merge_finished_analysis_staging(
        &options,
        &worker_ids,
        stats,
        progress_cb,
        analysis_started,
    )?;
    // Merge (correlation + index) done — advance to CorrelateAndIndex.
    {
        let mut ts = options
            .tier_state
            .lock()
            .map_err(|_| ImportAnalysisError::Other("tier state lock poisoned".to_string()))?;
        advance_tier(&mut ts);
    }
    Ok(result)
}
