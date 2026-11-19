//! Complete post-Ready artifact work under the import publication lease.
use super::context::ImportJobContext;
use std::time::Instant;
use transport::CommandError;

mod android;
mod progress;
mod source_facts;
use progress::AnalysisAuditProgress;

pub(super) fn finish(
    ctx: &mut ImportJobContext<'_>,
    ds: &domain::DataSource,
    source: &rusqlite::Connection,
    message: String,
) -> Result<String, CommandError> {
    let result: Result<String, CommandError> = (|| {
        ensure_active(ctx)?;
        let lease = ctx.runtime.as_ref().map(|r| r.publication_lease());
        let _source = lease.as_ref().map(|l| l.fence_connection(source));
        let message = if matches!(
            ctx.post_import_mode,
            crate::import_pipeline::options::PostImportMode::Detached
        ) {
            finish_detached_artifacts(ctx, ds, message)?
        } else {
            auto_extract_platform_artifacts(ctx, ds, source, message)?
        };
        ensure_active(ctx)?;
        super::phases::emit_data_source_ready(ctx, ds)?;
        Ok(message)
    })();
    if let Err(error) = &result {
        if error.category == "cancelled" {
            super::execute::mark_import_cancelling(&ctx.job_repo, ctx.job_id, &error.message);
            super::execute::emit_import_cancellation_state(
                ctx.event_sink(),
                ctx.job_id,
                transport::dto::CancellationStateDto::Cancelled,
                false,
                &error.message,
            );
        } else {
            super::runtime::record_failure(ctx, ds, error);
        }
    }
    result
}

fn auto_extract_platform_artifacts(
    ctx: &mut ImportJobContext<'_>,
    data_source: &domain::DataSource,
    source_conn: &rusqlite::Connection,
    mut message: String,
) -> Result<String, CommandError> {
    if ctx.import_config.platform == domain::DataSourcePlatform::Android {
        return android::analyze(ctx, data_source, message);
    }
    let Some((platform_label, categories)) = analysis_plan(ctx) else {
        return Ok(message);
    };
    ctx.report_job_progress(
        96,
        &format!("{platform_label} artifact analysis started automatically"),
    )?;
    let mut runtime = analysis_core::analysis_service::AnalysisSourceReadRuntime::default();
    if let Some(source) = &ctx.runtime {
        runtime = runtime.with_publication(source.publication_lease());
    }
    let analysis_started = Instant::now();
    let event_sink = ctx.options.event_sink;
    let job_id = ctx.job_id.0.as_str();
    let mut audit_progress =
        AnalysisAuditProgress::new(event_sink, job_id, analysis_started, ctx.profiler.as_ref());
    let run = analysis_core::analysis_service::run_import_source_extraction(
        analysis_core::analysis_service::ImportSourceExtractionRequest {
            case_conn: ctx.conn,
            case_root: ctx.case_root,
            case_id: ctx.case_id,
            source: &data_source.id,
            categories,
            runtime: &runtime,
            cancel_token: ctx.options.cancel_token.clone(),
            run_id: job_id,
        },
        |progress| audit_progress.emit(progress),
    );
    ensure_active(ctx)?;
    let execution = run.map_err(CommandError::from_typed_service_error)?;
    if let Some(profiler) = &ctx.profiler {
        profiler
            .record_analysis_result(
                &ctx.case_id.0,
                &data_source.id.0,
                job_id,
                execution.retryable_failure_count,
                &execution.dto,
            )
            .map_err(CommandError::from_typed_service_error)?;
    }
    let run = execution.dto;
    super::emit::emit_job_progress(
                event_sink,
                job_id,
                96,
                &format!(
                    "analysis-audit: phase=summary discoveryMs={} processingMs={} persistenceMs={} sourceReads={} sourceReadMs={} retryableFailures={} warnings={} fsReads={:?} radosReads={:?} totalMs={}",
                    execution.discovery_elapsed_ms,
                    execution.processing_elapsed_ms,
                    execution.persistence_elapsed_ms,
                    execution.source_read_count,
                    execution.source_read_elapsed_ms,
                    execution.retryable_failure_count,
                    run.warnings.len(),
                    execution.filesystem_read_metrics,
                    execution.rados_read_metrics,
                    analysis_started.elapsed().as_millis()
                ),
            );
    let warnings = run
        .warnings
        .len()
        .saturating_add(source_facts::cache_linux_facts(
            ctx,
            data_source,
            source_conn,
        ));
    record_analysis_outcome(ctx, warnings, execution.retryable_failure_count)?;
    message.push_str(&format!(
                "; {platform_label} artifacts auto-analyzed: artifacts={} timeline={} warnings={} failed={}",
                run.artifact_count,
                run.timeline_event_count,
                warnings,
                execution.retryable_failure_count
            ));
    Ok(message)
}

fn ensure_active(ctx: &ImportJobContext<'_>) -> Result<(), CommandError> {
    if let Some(profiler) = &ctx.profiler {
        profiler
            .check_output()
            .map_err(CommandError::from_typed_service_error)?;
    }
    if ctx.cancel_requested() {
        return Err(CommandError::cancelled("Import cancelled by user"));
    }
    super::runtime::validate_current(ctx.runtime.as_deref())
}

fn record_analysis_outcome(
    ctx: &mut ImportJobContext<'_>,
    warning_count: usize,
    failed_count: u64,
) -> Result<(), CommandError> {
    if warning_count == 0 && failed_count == 0 {
        return Ok(());
    }
    ctx.counts.add_warnings(warning_count);
    ctx.counts
        .add_failed(u32::try_from(failed_count).unwrap_or(u32::MAX));
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

/// Extraction categories consumed by `analysis_service::run_import_source_extraction`
/// for a platform. Android uses its own analysis entry and returns `None` here;
/// other platforms have no host-platform artifact extraction. Shared by the
/// inline auto-extractor and the detached post-import scheduler so category
/// lists are not hard-coded twice.
pub fn artifact_extraction_categories(
    platform: domain::DataSourcePlatform,
) -> Option<&'static [&'static str]> {
    match platform {
        domain::DataSourcePlatform::Windows => Some(
            &[
                "Registry",
                "BrowserHistory",
                "Email",
                "EventLogs",
                "ProgramExecution",
                "UserActivity",
                "RecycleBin",
                "Thumbnails",
                "ResourceUsage",
                "PluginArtifacts",
            ][..],
        ),
        domain::DataSourcePlatform::Linux => Some(&["LinuxArtifacts"][..]),
        domain::DataSourcePlatform::Android | domain::DataSourcePlatform::Unknown => None,
    }
}

/// Whether host-platform artifact extraction applies to this source: supported
/// platform, filesystem content, standalone (not an evidence-set member), and
/// not deferred. Android qualifies for its own entry; this guard mirrors the
/// inline `android::analyze` preconditions.
fn detached_artifacts_eligible(ctx: &ImportJobContext<'_>) -> bool {
    matches!(
        ctx.import_config.platform,
        domain::DataSourcePlatform::Windows
            | domain::DataSourcePlatform::Linux
            | domain::DataSourcePlatform::Android
    ) && matches!(
        ctx.content_kind,
        crate::import_pipeline::context::ImportContentKind::Filesystem
    ) && ctx.import_config.import_set.is_none()
        && !ctx.options.defer_linux_artifacts
}

/// Detached catalog-only post-ready: skip inline extraction, queue (or mark
/// not-applicable) the artifacts capability, then emit `data-source-ready`
/// immediately so catalog availability is not blocked by artifact work.
fn finish_detached_artifacts(
    ctx: &mut ImportJobContext<'_>,
    ds: &domain::DataSource,
    message: String,
) -> Result<String, CommandError> {
    let (state, detail) = if detached_artifacts_eligible(ctx) {
        (
            crate::import_pipeline::capability::QUEUED,
            Some("queued for post-import artifact extraction"),
        )
    } else {
        (
            crate::import_pipeline::capability::NOT_APPLICABLE,
            Some("artifact extraction is not applicable for this source"),
        )
    };
    crate::import_pipeline::capability::mark_capability(
        ctx.conn,
        ctx.case_id,
        &ds.id,
        "artifacts",
        state,
        detail,
        None,
    )?;
    Ok(message)
}

fn analysis_plan(ctx: &ImportJobContext<'_>) -> Option<(&'static str, &'static [&'static str])> {
    let should_run = matches!(
        ctx.import_config.platform,
        domain::DataSourcePlatform::Linux | domain::DataSourcePlatform::Windows
    ) && matches!(
        ctx.content_kind,
        crate::import_pipeline::context::ImportContentKind::Filesystem
    ) && ctx.import_config.import_set.is_none()
        && !ctx.options.defer_linux_artifacts;
    if !should_run {
        return None;
    }
    let label = match ctx.import_config.platform {
        domain::DataSourcePlatform::Linux => "Linux",
        domain::DataSourcePlatform::Windows => "Windows",
        _ => unreachable!("platform was checked above"),
    };
    Some((
        label,
        artifact_extraction_categories(ctx.import_config.platform)?,
    ))
}
