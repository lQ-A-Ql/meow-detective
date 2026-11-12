//! Complete post-Ready artifact work under the import publication lease.
use super::context::ImportJobContext;
use std::sync::atomic::Ordering;
use std::time::Instant;
use transport::CommandError;

pub(super) fn finish(
    ctx: &mut ImportJobContext<'_>,
    ds: &domain::DataSource,
    source: &rusqlite::Connection,
    message: String,
) -> Result<String, CommandError> {
    let result = (|| {
        super::runtime::validate_current(ctx.runtime.as_deref())?;
        let lease = ctx.runtime.as_ref().map(|r| r.publication_lease());
        let _source = lease.as_ref().map(|l| l.fence_connection(source));
        let message = auto_extract_platform_artifacts(ctx, ds, source, message)?;
        super::phases::emit_data_source_ready(ctx, ds)?;
        Ok(message)
    })();
    if let Err(error) = &result {
        super::runtime::record_failure(ctx, ds, error);
    }
    result
}

fn auto_extract_platform_artifacts(
    ctx: &mut ImportJobContext<'_>,
    data_source: &domain::DataSource,
    source_conn: &rusqlite::Connection,
    mut message: String,
) -> Result<String, CommandError> {
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
    let event_sink = ctx.event_sink();
    let job_id = ctx.job_id.0.as_str();
    let mut audit_progress = AnalysisAuditProgress::new(event_sink, job_id, analysis_started);
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
    match run {
        Ok(execution) => {
            let run = execution.dto;
            super::emit::emit_job_progress(
                event_sink,
                job_id,
                96,
                &format!(
                    "analysis-audit: phase=summary discoveryMs={} processingMs={} persistenceMs={} sourceReads={} sourceReadMs={} retryableFailures={} fsReads={:?} radosReads={:?} totalMs={}",
                    execution.discovery_elapsed_ms,
                    execution.processing_elapsed_ms,
                    execution.persistence_elapsed_ms,
                    execution.source_read_count,
                    execution.source_read_elapsed_ms,
                    execution.retryable_failure_count,
                    execution.filesystem_read_metrics,
                    execution.rados_read_metrics,
                    analysis_started.elapsed().as_millis()
                ),
            );
            let facts = ::analysis_core::linux_evidence_facts::collect_linux_evidence_facts(
                ctx.conn,
                ctx.case_root,
                ctx.case_id,
                &data_source.id,
            );
            if let Err(error) = ::analysis_core::linux_evidence_facts::persist_linux_evidence_facts(
                source_conn,
                &data_source.id,
                &facts,
            ) {
                tracing::warn!(data_source_id = %data_source.id.0, %error, "Linux evidence facts cache could not be persisted");
            }
            message.push_str(&format!(
                "; {platform_label} artifacts auto-analyzed: artifacts={} timeline={}",
                run.artifact_count, run.timeline_event_count
            ));
        }
        Err(error) => {
            ctx.counts.warning_count = ctx.counts.warning_count.saturating_add(1);
            ctx.counts.failed_count = ctx.counts.failed_count.saturating_add(1);
            ctx.job_repo
                .update_outcome_counts(
                    ctx.job_id,
                    ctx.counts.warning_count,
                    ctx.counts.skipped_count,
                    ctx.counts.failed_count,
                    true,
                )
                .map_err(CommandError::from_service_error)?;
            message.push_str(&format!(
                "; {platform_label} artifact auto-analysis partial: {error}"
            ));
        }
    }
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
        && !ctx.options.defer_linux_artifacts
        && !ctx.options.cancel_token.load(Ordering::Relaxed);
    if !should_run {
        return None;
    }
    match ctx.import_config.platform {
        domain::DataSourcePlatform::Linux => Some(("Linux", &["LinuxArtifacts"][..])),
        domain::DataSourcePlatform::Windows => Some((
            "Windows",
            &["Registry", "BrowserHistory", "Email", "EventLogs"][..],
        )),
        _ => unreachable!("platform was checked above"),
    }
}

struct AnalysisAuditProgress<'a> {
    event_sink: Option<&'a dyn super::emit::ImportEventSink>,
    job_id: &'a str,
    started: Instant,
    last_phase: Option<transport::dto::AnalysisExtractionPhaseDto>,
}

impl<'a> AnalysisAuditProgress<'a> {
    fn new(
        event_sink: Option<&'a dyn super::emit::ImportEventSink>,
        job_id: &'a str,
        started: Instant,
    ) -> Self {
        Self {
            event_sink,
            job_id,
            started,
            last_phase: None,
        }
    }

    fn emit(&mut self, progress: transport::dto::AnalysisExtractionProgressDto) {
        if self.last_phase != Some(progress.phase) {
            self.last_phase = Some(progress.phase);
            super::emit::emit_job_progress(
                self.event_sink,
                self.job_id,
                96,
                &format!(
                    "analysis-audit: phase={:?} elapsedMs={}",
                    progress.phase,
                    self.started.elapsed().as_millis()
                ),
            );
        }
        if progress.phase == transport::dto::AnalysisExtractionPhaseDto::Completed {
            super::emit::emit_job_progress(
                self.event_sink,
                self.job_id,
                96,
                &format!(
                    "analysis-audit: category={} candidates={} processed={} artifacts={} timeline={} checkpoints={} elapsedMs={}",
                    progress.category,
                    progress.total_candidates,
                    progress.processed_candidates,
                    progress.artifact_count,
                    progress.timeline_event_count,
                    progress.checkpoint_hit_count,
                    self.started.elapsed().as_millis()
                ),
            );
        }
    }
}
