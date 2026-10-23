use std::{
    path::{Path, PathBuf},
    sync::{atomic::AtomicBool, Arc},
};

use persistence_sqlite::repositories::job_repo::JobRepo;
use tauri::AppHandle;
use transport::CommandError;

use crate::{
    events::event_bridge,
    state::{TaskManager, TaskScope},
};

const CASE_PROJECTION_JOB_KIND: &str = "Case projections";

pub(crate) fn schedule(
    task_manager: &Arc<TaskManager>,
    app: &AppHandle,
    case_id: &str,
    case_root: &Path,
) -> Result<Option<String>, CommandError> {
    let db_path = db_path_for_root(case_root);
    let connection = app_services::connection::open_existing_case_db(&db_path)
        .map_err(CommandError::from_typed_service_error)?;
    let repo = JobRepo::new(&connection);
    if has_active_projection_build(&repo)? {
        return Ok(None);
    }
    let job_id = repo
        .create(case_id, CASE_PROJECTION_JOB_KIND)
        .map_err(CommandError::from_typed_service_error)?;
    repo.update_progress(&job_id, 1, "Queued case projection build")
        .map_err(CommandError::from_typed_service_error)?;
    drop(connection);

    let task_id = job_id.0.clone();
    let worker_id = task_id.clone();
    let worker_app = app.clone();
    let worker_root = case_root.to_path_buf();
    let worker_case_id = case_id.to_string();
    let worker_db_path = db_path.clone();
    let cancel_token = Arc::new(AtomicBool::new(false));
    let worker_cancel = Arc::clone(&cancel_token);
    if let Err(error) = task_manager.spawn_scoped(
        task_id.clone(),
        TaskScope::case(case_id, task_id.clone()),
        cancel_token,
        move || {
            run_worker(
                &worker_app,
                &worker_db_path,
                &worker_case_id,
                &worker_root,
                &worker_id,
                &worker_cancel,
            )
        },
    ) {
        fail_registration(&db_path, &job_id, &error.to_string())?;
        return Err(CommandError::internal(format!(
            "Case projection task registration failed: {error}"
        )));
    }
    event_bridge::emit_job_created(app, &task_id, CASE_PROJECTION_JOB_KIND);
    Ok(Some(task_id))
}

fn fail_registration(
    db_path: &Path,
    job_id: &domain::JobId,
    detail: &str,
) -> Result<(), CommandError> {
    let connection = app_services::connection::open_existing_case_db(db_path)
        .map_err(CommandError::from_typed_service_error)?;
    JobRepo::new(&connection)
        .fail(job_id, detail)
        .map_err(CommandError::from_typed_service_error)
}

fn has_active_projection_build(repo: &JobRepo<'_>) -> Result<bool, CommandError> {
    let jobs = repo
        .list_recent(infrastructure::constants::JOB_LIST_LIMIT)
        .map_err(CommandError::from_typed_service_error)?;
    Ok(jobs.into_iter().any(|job| {
        job.kind == CASE_PROJECTION_JOB_KIND
            && matches!(job.status.as_str(), "pending" | "running" | "cancelling")
    }))
}

fn run_worker(
    app: &AppHandle,
    db_path: &Path,
    case_id: &str,
    case_root: &Path,
    job_id: &str,
    cancel: &AtomicBool,
) -> Result<(), String> {
    let job = domain::JobId(job_id.to_string());
    let connection = app_services::connection::open_existing_case_db(db_path)
        .map_err(|error| error.to_string())?;
    let repo = JobRepo::new(&connection);
    if !repo.start(&job).map_err(|error| error.to_string())? {
        return Err("Case projection job was cancelled before execution".to_string());
    }
    repo.update_progress(&job, 5, "Building case projections")
        .map_err(|error| error.to_string())?;
    event_bridge::emit_job_started(app, job_id, "Building case projections");
    match app_services::case_projection_service::rebuild_case_projections(
        &connection,
        case_root,
        case_id,
        cancel,
    ) {
        Ok(report) => {
            let detail = format!("Case projections ready at {}", report.completed_at);
            repo.complete_committed(&job, &detail)
                .map_err(|error| error.to_string())?;
            event_bridge::emit_job_progress(app, job_id, 100, &detail);
            event_bridge::emit_job_completed(app, job_id, &detail);
            Ok(())
        }
        Err(app_services::case_projection_service::CaseProjectionError::Cancelled) => {
            repo.cancel(&job, "Case projection build cancelled")
                .map_err(|error| error.to_string())?;
            event_bridge::emit_job_cancelled(app, job_id, "Case projection build cancelled");
            Err("Case projection build cancelled".to_string())
        }
        Err(error) => {
            let detail = error.to_string();
            repo.fail(&job, &detail)
                .map_err(|db_error| db_error.to_string())?;
            event_bridge::emit_job_failed(app, job_id, &detail);
            Err(detail)
        }
    }
}

fn db_path_for_root(case_root: &Path) -> PathBuf {
    case_root.join("app.db")
}
