//! Complete post-Ready artifact work under the import publication lease.
use super::context::ImportJobContext;
use std::sync::atomic::Ordering;
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
        let message = auto_extract_linux_artifacts(ctx, ds, source, message)?;
        super::phases::emit_data_source_ready(ctx, ds)?;
        Ok(message)
    })();
    if let Err(error) = &result {
        super::runtime::record_failure(ctx, ds, error);
    }
    result
}

fn auto_extract_linux_artifacts(
    ctx: &mut ImportJobContext<'_>,
    data_source: &domain::DataSource,
    source_conn: &rusqlite::Connection,
    mut message: String,
) -> Result<String, CommandError> {
    let should_run = ctx.import_config.platform == domain::DataSourcePlatform::Linux
        && matches!(
            ctx.content_kind,
            crate::import_pipeline::context::ImportContentKind::Filesystem
        )
        && ctx.import_config.import_set.is_none()
        && !ctx.options.defer_linux_artifacts
        && !ctx.options.cancel_token.load(Ordering::Relaxed);
    if !should_run {
        return Ok(message);
    }
    ctx.report_job_progress(96, "Linux artifact analysis started automatically")?;
    let mut runtime = crate::analysis_service::AnalysisSourceReadRuntime::default();
    if let Some(source) = &ctx.runtime {
        runtime = runtime.with_publication(source.publication_lease());
    }
    match crate::analysis_service::run_import_source_extraction(
        ctx.conn,
        ctx.case_root,
        ctx.case_id,
        &data_source.id,
        &runtime,
        ctx.options.cancel_token.clone(),
    ) {
        Ok(run) => {
            let facts = crate::cluster_service::collect_linux_evidence_facts(
                ctx.conn,
                ctx.case_root,
                ctx.case_id,
                &data_source.id,
            );
            if let Err(error) = crate::cluster_service::persist_linux_evidence_facts(
                source_conn,
                &data_source.id,
                &facts,
            ) {
                tracing::warn!(data_source_id = %data_source.id.0, %error, "Linux evidence facts cache could not be persisted");
            }
            message.push_str(&format!(
                "; Linux artifacts auto-analyzed: artifacts={} timeline={}",
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
            message.push_str(&format!("; Linux artifact auto-analysis partial: {error}"));
        }
    }
    Ok(message)
}
