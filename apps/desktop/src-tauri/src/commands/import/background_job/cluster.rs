mod completion;
mod execution;
mod failures;
mod ledger;

use super::{
    gate::acquire_import_slot,
    status::{cancel_job, fail_linux_evidence_set_job},
    types::{BackgroundLinuxEvidenceSetImportJob, BrowseableEvidenceSetImport},
};
use crate::events::event_bridge;
use crate::state::TaskManager;
use app_services::cluster_service;
use persistence_sqlite::repositories::job_repo::JobRepo;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tauri::AppHandle;
use transport::CommandError;

use ledger::record_cluster_phase;

pub(crate) fn run_background_linux_evidence_set_import_with_scheduler(
    job: BackgroundLinuxEvidenceSetImportJob,
    app: Option<&AppHandle>,
    cancel_token: Arc<AtomicBool>,
    task_manager: Option<Arc<TaskManager>>,
) -> Result<Option<BrowseableEvidenceSetImport>, CommandError> {
    let connection = app_services::connection::open_existing_case_db(&job.db_path)
        .map_err(CommandError::from_typed_service_error)?;
    let job_repo = JobRepo::new(&connection);
    let projection_task_manager = task_manager.as_ref().cloned();
    if cancel_token.load(Ordering::Relaxed) {
        cancel_job(
            &job_repo,
            &job.job_id,
            app,
            "Linux evidence-set import cancelled by user",
        );
        return Ok(None);
    }
    let _import_slot = acquire_import_slot(&job_repo, &job.job_id, app, &cancel_token)?;
    if !start_cluster_job(&job_repo, &job.job_id, app)? {
        return Ok(None);
    }
    emit_cluster_started(app, &job.job_id);
    if let Err(error) = initialize_evidence_set(&connection, &job_repo, &job, app) {
        record_cluster_phase(
            &connection,
            &job,
            "initialize",
            false,
            Some(&error.message),
            0,
            0,
        );
        return Err(error);
    }
    record_cluster_phase(&connection, &job, "initialize", true, None, 0, 0);
    let Some(summary) = execution::import_members(
        &connection,
        &job_repo,
        &job,
        app,
        &cancel_token,
        task_manager,
    )?
    else {
        return Ok(None);
    };
    record_cluster_phase(
        &connection,
        &job,
        "member_import",
        summary.failed_count == 0,
        (summary.failed_count > 0).then_some("one or more members failed"),
        summary.ready_count,
        summary.failed_count,
    );
    execution::publish_evidence_set(
        &connection,
        &job_repo,
        &job,
        app,
        summary,
        cancel_token,
        projection_task_manager,
    )
}

fn start_cluster_job(
    job_repo: &JobRepo<'_>,
    job_id: &domain::JobId,
    app: Option<&AppHandle>,
) -> Result<bool, CommandError> {
    let started = job_repo
        .start(job_id)
        .map_err(CommandError::from_typed_service_error)?;
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

fn emit_cluster_started(app: Option<&AppHandle>, job_id: &domain::JobId) {
    if let Some(app) = app {
        event_bridge::emit_job_started(app, &job_id.0, "Linux evidence-set import started");
        event_bridge::emit_job_progress(app, &job_id.0, 5, "Linux evidence-set import started");
    }
}

fn initialize_evidence_set(
    connection: &rusqlite::Connection,
    job_repo: &JobRepo<'_>,
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
