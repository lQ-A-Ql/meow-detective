use super::super::{
    status::fail_job,
    types::{BackgroundLinuxEvidenceSetImportJob, EvidenceSetImportSummary},
};
use app_services::cluster_service;
use tauri::AppHandle;
use transport::CommandError;

pub(super) fn complete_evidence_set_import_with_failures(
    connection: &rusqlite::Connection,
    job_repo: &super::JobRepo<'_>,
    job: &BackgroundLinuxEvidenceSetImportJob,
    app: Option<&AppHandle>,
    summary: EvidenceSetImportSummary,
    total_members: u32,
) -> Result<(), CommandError> {
    match cluster_service::materialize_linux_evidence_set_mdraid(
        connection,
        &job.case_root,
        &job.case_id,
        &job.plan,
    ) {
        Ok(sources) if !sources.is_empty() => tracing::info!(
            import_set_id = %job.plan.import_set_id,
            derived_sources = sources.len(),
            "Materialized nested MD RAID source catalogs after member import failure"
        ),
        Ok(_) => {}
        Err(error) => tracing::warn!(
            import_set_id = %job.plan.import_set_id,
            error = %error,
            "Nested MD RAID source catalog materialization failed after member import failure"
        ),
    }
    let message = format!(
        "Linux evidence-set import finished with failures: {}/{} image(s) ready, {} failed",
        summary.ready_count, total_members, summary.failed_count
    );
    cluster_service::update_linux_evidence_set_import_state(
        connection,
        &job.plan.import_set_id,
        "failed",
        summary.ready_count,
        summary.failed_count,
        Some(&message),
    )
    .map_err(CommandError::from_typed_service_error)?;
    job_repo
        .update_outcome_counts(&job.job_id, 0, 0, summary.failed_count, true)
        .map_err(CommandError::from_typed_service_error)?;
    tracing::warn!(
        import_set_id = %job.plan.import_set_id,
        members = total_members,
        imported = summary.ready_count,
        failed = summary.failed_count,
        summaries = ?summary.member_messages,
        "Linux evidence-set import completed with member failures"
    );
    fail_job(job_repo, &job.job_id, app, CommandError::internal(message))
}
