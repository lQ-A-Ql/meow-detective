use std::sync::{Arc, Mutex};
use std::time::Instant;

use transport::CommandError;

use crate::{import_analysis, source_db};

use crate::import_pipeline::context::ImportJobContext;
use crate::import_pipeline::execute::{
    emit_import_cancellation_state, is_import_cancelled_message, mark_import_cancelling,
};
use crate::import_pipeline::options::JobOutcomeCounts;
use crate::import_pipeline::profile::{
    elapsed_ms, emit_import_profile_progress, emit_phase_profile, post_import_counts_from_message,
};

pub(crate) fn run_analyze_phase(
    ctx: &mut ImportJobContext<'_>,
    data_source: &domain::DataSource,
) -> Result<String, CommandError> {
    if ctx.content_kind == crate::import_pipeline::context::ImportContentKind::CephBlueStoreMetadata
    {
        ctx.report_job_progress(94, "Ceph BlueStore metadata inventory completed")?;
        return Ok(
            "Ceph BlueStore label metadata inventoried; RADOS/PG/object reconstruction remains unsupported"
                .to_string(),
        );
    }
    ctx.report_job_progress(70, "Running post-import pipeline...")?;
    let started = Instant::now();
    let analysis_mode = effective_analysis_mode(ctx);
    let progress = |percent: u32, detail: &str| {
        emit_import_profile_progress(
            ctx.event_sink(),
            ctx.job_id,
            ctx.case_id,
            Some(&data_source.id),
            percent,
            detail,
            ctx.cancel_requested(),
        );
    };
    let result = import_analysis::run_post_import_pipeline_with_counts(
        build_analysis_options(ctx, data_source, analysis_mode),
        Some(&progress),
    );
    let (message, counts) = match result {
        Ok(result) => result,
        Err(error) => return Err(map_analysis_error(ctx, error)),
    };
    apply_analysis_counts(ctx, JobOutcomeCounts::from(counts))?;
    report_analysis_complete(ctx, data_source, &message, started.elapsed());
    Ok(message)
}

fn effective_analysis_mode(ctx: &ImportJobContext<'_>) -> import_analysis::ImportAnalysisMode {
    if ctx.import_config.is_image_backed() {
        return ctx.options.analysis_mode;
    }
    match ctx.options.analysis_mode {
        import_analysis::ImportAnalysisMode::MetadataOnly => {
            import_analysis::ImportAnalysisMode::BudgetedContent
        }
        mode => mode,
    }
}

fn build_analysis_options(
    ctx: &ImportJobContext<'_>,
    data_source: &domain::DataSource,
    analysis_mode: import_analysis::ImportAnalysisMode,
) -> import_analysis::PostImportPipelineOptions {
    let image_backed = ctx.import_config.is_image_backed();
    import_analysis::PostImportPipelineOptions {
        case_root: ctx.case_root.to_path_buf(),
        db_path: source_db::source_db_path(ctx.case_root, &data_source.id),
        case_id: ctx.case_id.0.clone(),
        data_source_id: data_source.id.clone(),
        platform: ctx.import_config.platform,
        index_dir: source_db::source_index_dir(ctx.case_root, &data_source.id),
        max_analysis_workers: ctx.options.max_analysis_workers,
        cancel_token: Arc::clone(ctx.options.cancel_token),
        enable_timeline_projection: !image_backed,
        enable_content_extraction: analysis_mode.allows_content(),
        enable_text_indexing: analysis_mode.allows_content(),
        analysis_mode,
        tier_state: Arc::new(Mutex::new(import_analysis::tier::TierStateMachine::new())),
    }
}

fn map_analysis_error(
    ctx: &mut ImportJobContext<'_>,
    error: import_analysis::PostImportPipelineError,
) -> CommandError {
    add_counts(ctx.counts, JobOutcomeCounts::from(error.counts));
    if ctx.cancel_requested() || is_import_cancelled_message(&error.message) {
        mark_import_cancelling(
            &ctx.job_repo,
            ctx.job_id,
            "Cancellation acknowledged during post-import analysis drain",
        );
        emit_import_cancellation_state(
            ctx.event_sink(),
            ctx.job_id,
            transport::dto::CancellationStateDto::Draining,
            false,
            "Cancellation acknowledged during post-import analysis drain",
        );
        CommandError::internal("Import cancelled by user")
    } else {
        CommandError::from_service_error(error.message)
    }
}

fn apply_analysis_counts(
    ctx: &mut ImportJobContext<'_>,
    counts: JobOutcomeCounts,
) -> Result<(), CommandError> {
    add_counts(ctx.counts, counts);
    ctx.job_repo
        .update_outcome_counts(
            ctx.job_id,
            ctx.counts.warning_count,
            ctx.counts.skipped_count,
            ctx.counts.failed_count,
            ctx.counts.is_partial(),
        )
        .map_err(CommandError::from_service_error)
}

fn add_counts(target: &mut JobOutcomeCounts, source: JobOutcomeCounts) {
    target.warning_count = target.warning_count.saturating_add(source.warning_count);
    target.skipped_count = target.skipped_count.saturating_add(source.skipped_count);
    target.failed_count = target.failed_count.saturating_add(source.failed_count);
}

fn report_analysis_complete(
    ctx: &ImportJobContext<'_>,
    data_source: &domain::DataSource,
    message: &str,
    elapsed: std::time::Duration,
) {
    let results = post_import_counts_from_message(message);
    emit_phase_profile(
        ctx.event_sink(),
        ctx.job_id,
        ctx.case_id,
        Some(&data_source.id),
        94,
        format!(
            "Post-import complete: phase=post-import elapsedMs={} timeline={} artifacts={} indexed={} rssMb={}",
            elapsed_ms(elapsed),
            results.timeline_events,
            results.artifact_count,
            results.indexed_count,
            import_analysis::current_rss_mb()
        ),
        ctx.cancel_requested(),
    );
}
