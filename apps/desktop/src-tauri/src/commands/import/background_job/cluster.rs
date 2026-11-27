mod completion;
mod completion_finish;
mod execution;
mod failures;
mod ledger;
mod lvm;
mod mdraid;
mod startup;
mod topology;

use super::{
    gate::acquire_import_slot,
    status::{cancel_job, fail_linux_evidence_set_job},
    types::{BackgroundLinuxEvidenceSetImportJob, BrowseableEvidenceSetImport},
};
use crate::state::TaskManager;
use persistence_sqlite::repositories::job_repo::JobRepo;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Instant;
use tauri::AppHandle;
use transport::CommandError;

use ledger::record_cluster_phase;

pub(super) type ClusterJobRepository<'a> = JobRepo<'a>;

pub(crate) fn run_background_linux_evidence_set_import_with_scheduler(
    job: BackgroundLinuxEvidenceSetImportJob,
    app: Option<&AppHandle>,
    cancel_token: Arc<AtomicBool>,
    task_manager: Option<Arc<TaskManager>>,
) -> Result<Option<BrowseableEvidenceSetImport>, CommandError> {
    let connection = app_services::connection::open_existing_case_db(&job.db_path)
        .map_err(app_services::connection::command_error_from_db)?;
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
    let _import_slot =
        acquire_import_slot(&job_repo, &job.job_id, &job.case_id.0, app, &cancel_token)?;
    if !startup::start_cluster_job(&job_repo, &job.job_id, app)? {
        return Ok(None);
    }
    startup::emit_cluster_started(app, &job.job_id);
    if let Err(error) = startup::begin_content_identity(&job_repo, app, &job) {
        return fail_linux_evidence_set_job(&job_repo, &job.job_id, app, None, error)
            .map(|()| None);
    }
    let collection_started = Instant::now();
    let collection_plan = match job.plan.prepare_shared_sources(&cancel_token) {
        Ok(plan) => plan,
        Err(error) => {
            return startup::handle_content_identity_failure(
                &connection,
                &job_repo,
                &job,
                app,
                CommandError::from_typed_service_error(error),
            )
        }
    };
    record_cluster_phase(
        &connection,
        &job,
        "content_identity",
        true,
        None,
        job.plan.members.len() as u32,
        0,
    );
    startup::log_content_identity_completed(&job, collection_started);
    if let Err(error) = startup::initialize_evidence_set(&connection, &job_repo, &job, app) {
        record_cluster_phase(
            &connection,
            &job,
            "member_setup",
            false,
            Some(&error.message),
            0,
            0,
        );
        return Err(error);
    }
    record_cluster_phase(&connection, &job, "member_setup", true, None, 0, 0);
    let Some(summary) = execution::import_members(
        &connection,
        &job_repo,
        &job,
        app,
        &cancel_token,
        collection_plan,
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
