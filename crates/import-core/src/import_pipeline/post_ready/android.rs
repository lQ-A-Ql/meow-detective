use super::super::context::{ImportContentKind, ImportJobContext};
use analysis_core::analysis_service::{
    run_source_android_analysis_with_cancel, AnalysisSourceReadRuntime,
};
use domain::DataSource;
use transport::CommandError;

pub(super) fn analyze(
    context: &mut ImportJobContext<'_>,
    source: &DataSource,
    mut message: String,
) -> Result<String, CommandError> {
    if context.content_kind != ImportContentKind::Filesystem
        || context.import_config.import_set.is_some()
        || context.options.defer_linux_artifacts
    {
        return Ok(message);
    }
    super::ensure_active(context)?;
    context.report_job_progress(96, "Android metadata analysis started automatically")?;
    let mut runtime = AnalysisSourceReadRuntime::default();
    if let Some(source_runtime) = &context.runtime {
        runtime = runtime.with_publication(source_runtime.publication_lease());
    }
    let result = run_source_android_analysis_with_cancel(
        context.conn,
        context.case_root,
        context.case_id,
        &source.id,
        &runtime,
        context.options.cancel_token.as_ref(),
    )
    .map_err(CommandError::from_typed_service_error)?;
    super::ensure_active(context)?;
    super::record_analysis_outcome(context, result.warnings.len(), 0)?;
    message.push_str(&format!(
        "; Android metadata auto-analyzed: facts={} packages={} warnings={}",
        result.device_fact_count,
        result.package_count,
        result.warnings.len()
    ));
    Ok(message)
}
