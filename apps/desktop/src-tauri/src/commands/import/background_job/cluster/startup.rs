use std::time::Instant;

use super::super::{
    status::{cancel_job, fail_linux_evidence_set_job},
    types::{BackgroundLinuxEvidenceSetImportJob, BrowseableEvidenceSetImport},
};
use crate::events::event_bridge;
use app_services::cluster_service;
use tauri::AppHandle;
use transport::CommandError;

use super::{ledger::record_cluster_phase, ClusterJobRepository};

pub(super) fn start_cluster_job(
    job_repo: &ClusterJobRepository<'_>,
    job_id: &domain::JobId,
    app: Option<&AppHandle>,
) -> Result<bool, CommandError> {
    let started = job_repo
        .start(job_id)
        .map_err(app_services::connection::command_error_from_db)?;
    if !started {
        cancel_job(
            job_repo,
            job_id,
            app,
            "Evidence-set import cancelled before execution",
        );
    }
    Ok(started)
}

pub(super) fn emit_cluster_started(app: Option<&AppHandle>, job_id: &domain::JobId) {
    if let Some(app) = app {
        event_bridge::emit_job_started(app, &job_id.0, "Linux evidence-set import started");
        event_bridge::emit_job_progress(app, &job_id.0, 5, "Linux evidence-set import started");
    }
}

pub(super) fn begin_content_identity(
    job_repo: &ClusterJobRepository<'_>,
    app: Option<&AppHandle>,
    job: &BackgroundLinuxEvidenceSetImportJob,
) -> Result<(), CommandError> {
    job_repo
        .update_progress(&job.job_id, 10, "Hashing shared source content")
        .map_err(app_services::connection::command_error_from_db)?;
    if let Some(app) = app {
        event_bridge::emit_job_progress(app, &job.job_id.0, 10, "Hashing shared source content");
    }
    tracing::info!(
        import_set_id = %job.plan.import_set_id,
        members = job.plan.members.len(),
        "Linux evidence-set content identity started"
    );
    Ok(())
}

pub(super) fn handle_content_identity_failure(
    connection: &rusqlite::Connection,
    job_repo: &ClusterJobRepository<'_>,
    job: &BackgroundLinuxEvidenceSetImportJob,
    app: Option<&AppHandle>,
    error: CommandError,
) -> Result<Option<BrowseableEvidenceSetImport>, CommandError> {
    record_cluster_phase(
        connection,
        job,
        "content_identity",
        false,
        Some(&error.message),
        0,
        0,
    );
    if error.category == "cancelled" {
        cancel_job(
            job_repo,
            &job.job_id,
            app,
            "Linux evidence-set content identity cancelled",
        );
        return Ok(None);
    }
    fail_linux_evidence_set_job(job_repo, &job.job_id, app, None, error).map(|()| None)
}

pub(super) fn log_content_identity_completed(
    job: &BackgroundLinuxEvidenceSetImportJob,
    started: Instant,
) {
    tracing::info!(
        import_set_id = %job.plan.import_set_id,
        members = job.plan.members.len(),
        elapsed_ms = started.elapsed().as_millis() as u64,
        "Linux evidence-set content identity completed"
    );
}

pub(super) fn initialize_evidence_set(
    connection: &rusqlite::Connection,
    job_repo: &ClusterJobRepository<'_>,
    job: &BackgroundLinuxEvidenceSetImportJob,
    app: Option<&AppHandle>,
) -> Result<(), CommandError> {
    if let Err(error) =
        cluster_service::register_linux_evidence_set_import(connection, &job.case_id, &job.plan)
    {
        return fail_linux_evidence_set_job(
            job_repo,
            &job.job_id,
            app,
            None,
            CommandError::from_typed_service_error(error),
        );
    }
    if let Err(error) =
        cluster_service::write_linux_evidence_set_manifest(&job.case_root, &job.plan)
    {
        return fail_linux_evidence_set_job(
            job_repo,
            &job.job_id,
            app,
            Some((connection, &job.plan.import_set_id, 0, 1)),
            CommandError::from_typed_service_error(error),
        );
    }
    if let Err(error) = cluster_service::update_linux_evidence_set_import_state(
        connection,
        &job.plan.import_set_id,
        "importing",
        0,
        0,
        None,
    ) {
        return fail_linux_evidence_set_job(
            job_repo,
            &job.job_id,
            app,
            Some((connection, &job.plan.import_set_id, 0, 1)),
            CommandError::from_typed_service_error(error),
        );
    }
    Ok(())
}
