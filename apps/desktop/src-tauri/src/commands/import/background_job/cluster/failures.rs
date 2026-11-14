use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use super::super::{
    status::{fail_job, update_import_set_state_logged},
    types::{BackgroundLinuxEvidenceSetImportJob, EvidenceSetImportSummary},
};
use crate::events::event_bridge;
use app_services::cluster_service;
use tauri::AppHandle;
use transport::CommandError;

use super::ledger::record_cluster_phase;
use super::mdraid::{record_mdraid_cancellation, MdRaidOutcome};

pub(super) fn complete_evidence_set_import_with_failures(
    connection: &rusqlite::Connection,
    job_repo: &super::JobRepo<'_>,
    job: &BackgroundLinuxEvidenceSetImportJob,
    app: Option<&AppHandle>,
    summary: EvidenceSetImportSummary,
    total_members: u32,
    cancel_token: &Arc<AtomicBool>,
) -> Result<(), CommandError> {
    let mdraid_sources = match materialize_mdraid_after_member_failures(
        connection,
        job_repo,
        job,
        app,
        &summary,
        total_members,
        cancel_token,
    ) {
        MdRaidOutcome::Ready(sources) => sources,
        MdRaidOutcome::Cancelled => return Ok(()),
        MdRaidOutcome::Failed(error) => {
            return fail_job(job_repo, &job.job_id, app, error);
        }
    };
    let detail = if mdraid_sources.is_empty() {
        format!(
            "Linux evidence-set import completed partially: {}/{} member image(s) ready",
            summary.ready_count, total_members
        )
    } else {
        format!(
            "Linux evidence-set import completed partially: {}/{} member image(s) ready; {} nested MD RAID catalog(s) materialized",
            summary.ready_count,
            total_members,
            mdraid_sources.len()
        )
    };
    cluster_service::update_linux_evidence_set_import_state(
        connection,
        &job.plan.import_set_id,
        "failed",
        summary.ready_count,
        summary.failed_count,
        Some(&detail),
    )
    .map_err(CommandError::from_typed_service_error)?;
    job_repo
        .update_outcome_counts(&job.job_id, summary.failed_count, 0, 0, true)
        .map_err(app_services::connection::command_error_from_db)?;
    job_repo
        .complete_if_active(&job.job_id, &detail)
        .map_err(app_services::connection::command_error_from_db)?;
    if let Some(app) = app {
        event_bridge::emit_job_completed(app, &job.job_id.0, &detail);
    }
    tracing::warn!(
        import_set_id = %job.plan.import_set_id,
        members = total_members,
        imported = summary.ready_count,
        failed = summary.failed_count,
        mdraid_sources = mdraid_sources.len(),
        summaries = ?summary.member_messages,
        "Linux evidence-set import completed partially with nested MD RAID catalogs materialized"
    );
    Ok(())
}

fn materialize_mdraid_after_member_failures(
    connection: &rusqlite::Connection,
    job_repo: &super::JobRepo<'_>,
    job: &BackgroundLinuxEvidenceSetImportJob,
    app: Option<&AppHandle>,
    summary: &EvidenceSetImportSummary,
    total_members: u32,
    cancel_token: &Arc<AtomicBool>,
) -> MdRaidOutcome {
    if let Some(app) = app {
        event_bridge::emit_job_progress(
            app,
            &job.job_id.0,
            80,
            "Materializing nested MD RAID catalog after member import failures",
        );
    }
    match cluster_service::materialize_linux_evidence_set_mdraid_with_cancel(
        connection,
        &job.case_root,
        &job.case_id,
        &job.plan,
        cancel_token,
    ) {
        Ok(sources) => {
            record_cluster_phase(
                connection,
                job,
                "mdraid_catalog",
                true,
                None,
                summary.ready_count,
                summary.failed_count,
            );
            tracing::info!(
                import_set_id = %job.plan.import_set_id,
                derived_sources = sources.len(),
                "Materialized nested MD RAID source catalogs after member import failure"
            );
            MdRaidOutcome::Ready(sources)
        }
        Err(_) if cancel_token.load(Ordering::Acquire) => {
            record_mdraid_cancellation(connection, job_repo, job, app, summary);
            MdRaidOutcome::Cancelled
        }
        Err(error) => {
            let detail = format!(
                "Linux evidence-set import finished with failures: {}/{} image(s) ready, {} failed; nested MD RAID catalog materialization failed: {error}",
                summary.ready_count, total_members, summary.failed_count
            );
            record_cluster_phase(
                connection,
                job,
                "mdraid_catalog",
                false,
                Some(&error.to_string()),
                summary.ready_count,
                summary.failed_count,
            );
            update_import_set_state_logged(
                connection,
                &job.plan.import_set_id,
                "failed",
                summary.ready_count,
                summary.failed_count,
                Some(&detail),
                "Failed to mark Linux evidence set failed after MD RAID materialization failure",
            );
            tracing::warn!(
                import_set_id = %job.plan.import_set_id,
                members = total_members,
                imported = summary.ready_count,
                failed = summary.failed_count,
                summaries = ?summary.member_messages,
                %error,
                "Linux evidence-set import completed with failures and an unavailable MD RAID catalog"
            );
            MdRaidOutcome::Failed(CommandError::from_typed_service_error(error))
        }
    }
}
