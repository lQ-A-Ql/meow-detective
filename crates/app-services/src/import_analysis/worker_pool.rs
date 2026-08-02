use super::budget::content_budget_for_mode;
use super::error::ImportAnalysisError;
use super::extractor_policy::validate_analysis_platform;
use super::finalize::{
    apply_search_stats, collect_done_worker_stats, discover_analysis_worker_ids,
    merge_finished_analysis_staging, prepare_analysis_staging_startup, rebuild_file_metadata_index,
    AnalysisStartupAction,
};
use super::options::{
    AnalysisProgressCallback, ImportAnalysisOptions, ImportAnalysisStats, JobOutcomeCounts,
    PostImportPipelineError, PostImportPipelineOptions, PostImportPipelineReport,
};
use super::progress::{bool_word, current_rss_mb, memory_hard_limit_exceeded};
use super::source_reader::prepare_derived_runtime;
use super::task_feed::{analysis_task_queue_bound, count_analysis_file_tasks};
use super::tier::advance_tier;
use super::worker_coordinator::{run_analysis_workers, AnalysisWorkerRunConfig};
use crate::runtime_resources::{default_memory_hard_limit_mb, default_memory_soft_limit_mb};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Instant;

pub fn run_post_import_pipeline_with_counts(
    options: PostImportPipelineOptions,
    progress_cb: Option<AnalysisProgressCallback<'_>>,
) -> Result<(String, JobOutcomeCounts), PostImportPipelineError> {
    let report = run_post_import_pipeline_report(options, progress_cb)?;
    Ok((report.message, report.counts))
}

pub fn run_post_import_pipeline_report(
    options: PostImportPipelineOptions,
    progress_cb: Option<AnalysisProgressCallback<'_>>,
) -> Result<PostImportPipelineReport, PostImportPipelineError> {
    let mut counts = initial_counts_for_platform(options.platform)?;
    let tier_state = Arc::clone(&options.tier_state);
    if post_import_disabled(&options) {
        let analysis_options = import_analysis_options(options, Arc::clone(&tier_state));
        return finish_disabled_post_import(&analysis_options, &tier_state, counts, progress_cb);
    }

    emit_post_import_scheduled(&options, progress_cb);
    let derived_runtime = match prepare_derived_runtime(&options) {
        Ok(runtime) => runtime,
        Err(message) => {
            counts.add_failed(1);
            return Err(PostImportPipelineError { message, counts });
        }
    };
    let analysis_options = import_analysis_options(options, Arc::clone(&tier_state));
    let stats = match run_import_analysis_staging_with_runtime(
        analysis_options,
        derived_runtime,
        progress_cb,
    ) {
        Ok(stats) => stats,
        Err(error) => return Err(post_import_failure(error, counts)),
    };
    finish_post_import_tiers(&tier_state, &counts)?;
    counts.warning_count = counts.warning_count.saturating_add(stats.warning_count);
    counts.skipped_count = counts.skipped_count.saturating_add(stats.skipped_count);
    counts.failed_count = counts.failed_count.saturating_add(stats.failed_count);
    Ok(PostImportPipelineReport {
        message: post_import_message(&stats, &counts),
        counts,
        stats,
    })
}

fn post_import_disabled(options: &PostImportPipelineOptions) -> bool {
    !options.enable_content_extraction && !options.enable_text_indexing
}

fn finish_disabled_post_import(
    options: &ImportAnalysisOptions,
    tier_state: &Arc<std::sync::Mutex<super::tier::TierStateMachine>>,
    mut counts: JobOutcomeCounts,
    progress_cb: Option<AnalysisProgressCallback<'_>>,
) -> Result<PostImportPipelineReport, PostImportPipelineError> {
    if let Some(cb) = progress_cb {
        cb(
            84,
            "Post-import metadata indexing: phase=search-index scheduling=running workerBudget=0 activeWorkers=0 queuedTasks=0 pendingTasks=unknown timeline=finalize content=disabled text=disabled contentDeferred=true textDeferred=true",
        );
    }
    let search_stats = rebuild_file_metadata_index(options, progress_cb)
        .map_err(|error| post_import_failure(error, counts.clone()))?;
    let mut stats = ImportAnalysisStats::default();
    apply_search_stats(&mut stats, search_stats);
    counts.skipped_count = counts.skipped_count.saturating_add(stats.skipped_count);
    counts.failed_count = counts.failed_count.saturating_add(stats.failed_count);
    finish_post_import_tiers(tier_state, &counts)?;
    let message = format!(
        "Timeline: scheduled for import finalization. Artifacts: 0. Index: {} indexed",
        stats.indexed_count
    );
    Ok(PostImportPipelineReport {
        message,
        counts,
        stats,
    })
}

fn emit_post_import_scheduled(
    options: &PostImportPipelineOptions,
    progress_cb: Option<AnalysisProgressCallback<'_>>,
) {
    let Some(cb) = progress_cb else {
        return;
    };
    let content_deferred =
        !options.enable_content_extraction || !options.analysis_mode.allows_content();
    let text_deferred = !options.enable_text_indexing || !options.analysis_mode.allows_content();
    cb(
        70,
        &format!(
            "Post-import analysis scheduled: phase=analysis-start scheduling=queued mode={} workerBudget={} activeWorkers=0 queuedTasks=0 pendingTasks=unknown contentDeferred={} textDeferred={}",
            options.analysis_mode.as_str(),
            scheduled_worker_count(options.max_analysis_workers),
            bool_word(content_deferred),
            bool_word(text_deferred)
        ),
    );
}

fn import_analysis_options(
    options: PostImportPipelineOptions,
    tier_state: Arc<std::sync::Mutex<super::tier::TierStateMachine>>,
) -> ImportAnalysisOptions {
    ImportAnalysisOptions {
        case_root: options.case_root,
        db_path: options.db_path,
        case_id: options.case_id,
        data_source_id: options.data_source_id,
        platform: options.platform,
        index_dir: options.index_dir,
        max_analysis_workers: options.max_analysis_workers,
        cancel_token: options.cancel_token,
        enable_content_extraction: options.enable_content_extraction,
        enable_text_indexing: options.enable_text_indexing,
        analysis_mode: options.analysis_mode,
        content_budget: content_budget_for_mode(options.analysis_mode),
        memory_soft_limit_mb: default_memory_soft_limit_mb(),
        memory_hard_limit_mb: default_memory_hard_limit_mb(),
        tier_state,
    }
}

fn finish_post_import_tiers(
    tier_state: &Arc<std::sync::Mutex<super::tier::TierStateMachine>>,
    counts: &JobOutcomeCounts,
) -> Result<(), PostImportPipelineError> {
    let mut state = tier_state.lock().map_err(|_| PostImportPipelineError {
        message: "tier state lock poisoned".to_string(),
        counts: counts.clone(),
    })?;
    while advance_tier(&mut state).is_some() {}
    Ok(())
}

fn post_import_failure(
    error: ImportAnalysisError,
    mut counts: JobOutcomeCounts,
) -> PostImportPipelineError {
    counts.add_warnings(1);
    let message = error.to_string();
    if message.to_ascii_lowercase().contains("cancel") {
        counts.add_skipped(1);
    } else {
        counts.add_failed(1);
    }
    PostImportPipelineError { message, counts }
}

fn post_import_message(stats: &ImportAnalysisStats, counts: &JobOutcomeCounts) -> String {
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
    message
}

pub fn default_analysis_worker_count() -> usize {
    crate::import_scheduler::default_cpu_budget()
}

pub fn resolve_analysis_worker_count(max_analysis_workers: Option<usize>) -> usize {
    crate::import_scheduler::resolve_analysis_worker_count(max_analysis_workers)
}

pub fn resolve_analysis_worker_count_for_memory(
    max_analysis_workers: Option<usize>,
    rss_mb: u64,
    memory_soft_limit_mb: u64,
) -> usize {
    crate::import_scheduler::resolve_analysis_worker_count_for_memory(
        max_analysis_workers,
        rss_mb,
        memory_soft_limit_mb,
    )
}

pub fn run_import_analysis_staging(
    options: ImportAnalysisOptions,
    progress_cb: Option<AnalysisProgressCallback<'_>>,
) -> Result<ImportAnalysisStats, ImportAnalysisError> {
    run_import_analysis_staging_with_runtime(options, None, progress_cb)
}

fn run_import_analysis_staging_with_runtime(
    options: ImportAnalysisOptions,
    derived_runtime: Option<Arc<crate::ceph_reconstruction::DerivedRbdRuntime>>,
    progress_cb: Option<AnalysisProgressCallback<'_>>,
) -> Result<ImportAnalysisStats, ImportAnalysisError> {
    let analysis_started = validated_analysis_start(options.platform)?;
    let setup = AnalysisRunSetup::for_options(&options);
    let startup_action = prepare_analysis_staging_startup(
        &options,
        &setup.worker_ids,
        setup.worker_count,
        progress_cb,
    )?;
    if startup_action == AnalysisStartupAction::AlreadyMerged {
        return finish_already_merged_analysis(&options, progress_cb);
    }

    let estimated = count_analysis_file_tasks(&options.db_path, &options.data_source_id)?;
    ensure_analysis_memory_available(&options, setup.memory_hard_limit_mb)?;
    emit_analysis_start(&options, &setup, estimated, progress_cb);
    advance_analysis_tier(&options)?;
    if startup_action == AnalysisStartupAction::MergeOnly {
        return finish_merge_only_analysis(
            &options,
            &setup.worker_ids,
            progress_cb,
            analysis_started,
        );
    }

    let mut stats = run_analysis_workers(
        &options,
        AnalysisWorkerRunConfig {
            worker_ids: &setup.worker_ids,
            derived_runtime,
            estimated,
            memory_soft_limit_mb: setup.memory_soft_limit_mb,
            memory_hard_limit_mb: setup.memory_hard_limit_mb,
            analysis_started,
        },
        progress_cb,
    )?;
    advance_analysis_tier(&options)?;
    if options.cancel_token.load(Ordering::Relaxed) {
        stats.warning_count = stats.warning_count.saturating_add(1);
        return Err(ImportAnalysisError::Other(
            "Import analysis cancelled by user".to_string(),
        ));
    }
    let result = merge_finished_analysis_staging(
        &options,
        &setup.worker_ids,
        stats,
        progress_cb,
        analysis_started,
    )?;
    advance_analysis_tier(&options)?;
    Ok(result)
}

struct AnalysisRunSetup {
    worker_count: usize,
    worker_ids: Vec<usize>,
    memory_soft_limit_mb: u64,
    memory_hard_limit_mb: u64,
}

impl AnalysisRunSetup {
    fn for_options(options: &ImportAnalysisOptions) -> Self {
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
        let worker_count = resolve_analysis_worker_count_for_memory(
            options.max_analysis_workers,
            current_rss_mb(),
            memory_soft_limit_mb,
        );
        Self {
            worker_count,
            worker_ids: (0..worker_count).collect(),
            memory_soft_limit_mb,
            memory_hard_limit_mb,
        }
    }
}

fn scheduled_worker_count(max_analysis_workers: Option<usize>) -> usize {
    resolve_analysis_worker_count_for_memory(
        max_analysis_workers,
        current_rss_mb(),
        default_memory_soft_limit_mb(),
    )
}

fn finish_already_merged_analysis(
    options: &ImportAnalysisOptions,
    progress_cb: Option<AnalysisProgressCallback<'_>>,
) -> Result<ImportAnalysisStats, ImportAnalysisError> {
    if let Some(cb) = progress_cb {
        cb(
            94,
            "Analysis staging already merged; skipping analysis resume.",
        );
    }
    let worker_ids = discover_analysis_worker_ids(&options.case_root, &options.data_source_id.0)?;
    let mut stats = collect_done_worker_stats(options, &worker_ids)?;
    let search_stats = rebuild_file_metadata_index(options, progress_cb)?;
    apply_search_stats(&mut stats, search_stats);
    finish_analysis_tiers(options)?;
    Ok(stats)
}

fn ensure_analysis_memory_available(
    options: &ImportAnalysisOptions,
    memory_hard_limit_mb: u64,
) -> Result<(), ImportAnalysisError> {
    if !memory_hard_limit_exceeded(memory_hard_limit_mb) {
        return Ok(());
    }
    options.cancel_token.store(true, Ordering::Relaxed);
    Err(ImportAnalysisError::Other(format!(
        "Import analysis memory hard limit exceeded before start: rssMb={} hardLimitMb={}",
        current_rss_mb(),
        memory_hard_limit_mb
    )))
}

fn emit_analysis_start(
    options: &ImportAnalysisOptions,
    setup: &AnalysisRunSetup,
    estimated: u64,
    progress_cb: Option<AnalysisProgressCallback<'_>>,
) {
    let Some(cb) = progress_cb else {
        return;
    };
    cb(
        72,
        &format!(
            "Analysis staging: phase=analysis-start scheduling=queued mode={} workers={} workerBudget={} activeWorkers=0 queuedTasks=0 pendingTasks={} queueBound={} content={} text={} contentDeferred={} textDeferred={} rssMb={}",
            options.analysis_mode.as_str(),
            setup.worker_count,
            setup.worker_count,
            estimated,
            analysis_task_queue_bound(setup.worker_count),
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
            bool_word(
                !options.enable_content_extraction || !options.analysis_mode.allows_content()
            ),
            bool_word(!options.enable_text_indexing || !options.analysis_mode.allows_content()),
            current_rss_mb()
        ),
    );
}

fn finish_merge_only_analysis(
    options: &ImportAnalysisOptions,
    worker_ids: &[usize],
    progress_cb: Option<AnalysisProgressCallback<'_>>,
    analysis_started: Instant,
) -> Result<ImportAnalysisStats, ImportAnalysisError> {
    advance_analysis_tiers(options, 2)?;
    let stats = collect_done_worker_stats(options, worker_ids)?;
    let result =
        merge_finished_analysis_staging(options, worker_ids, stats, progress_cb, analysis_started)?;
    advance_analysis_tier(options)?;
    Ok(result)
}

fn advance_analysis_tier(options: &ImportAnalysisOptions) -> Result<(), ImportAnalysisError> {
    advance_analysis_tiers(options, 1)
}

fn advance_analysis_tiers(
    options: &ImportAnalysisOptions,
    count: usize,
) -> Result<(), ImportAnalysisError> {
    let mut state = options
        .tier_state
        .lock()
        .map_err(|_| ImportAnalysisError::Other("tier state lock poisoned".to_string()))?;
    for _ in 0..count {
        advance_tier(&mut state);
    }
    Ok(())
}

fn finish_analysis_tiers(options: &ImportAnalysisOptions) -> Result<(), ImportAnalysisError> {
    let mut state = options
        .tier_state
        .lock()
        .map_err(|_| ImportAnalysisError::Other("tier state lock poisoned".to_string()))?;
    while advance_tier(&mut state).is_some() {}
    Ok(())
}

fn initial_counts_for_platform(
    platform: domain::DataSourcePlatform,
) -> Result<JobOutcomeCounts, PostImportPipelineError> {
    validate_analysis_platform(platform)
        .map(|()| JobOutcomeCounts::default())
        .map_err(|error| {
            let mut counts = JobOutcomeCounts::default();
            counts.add_failed(1);
            PostImportPipelineError {
                message: error.to_string(),
                counts,
            }
        })
}

fn validated_analysis_start(
    platform: domain::DataSourcePlatform,
) -> Result<Instant, ImportAnalysisError> {
    validate_analysis_platform(platform)?;
    Ok(Instant::now())
}
