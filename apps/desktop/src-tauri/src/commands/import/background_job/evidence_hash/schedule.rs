use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use app_services::hash_service::evidence_jobs::{
    create_hash_job_if_absent, list_pending_hash_sources, settle_registration_failure,
    EVIDENCE_HASH_JOB_KIND,
};
use domain::{CaseId, DataSourceId, JobId};
use tauri::AppHandle;
use transport::CommandError;

use super::run_background_evidence_hash;
use crate::events::event_bridge;
use crate::state::{TaskManager, TaskRegistrationError, TaskScope};

const HASH_TASK_STACK_BYTES: usize = 16 * 1024 * 1024;

pub(crate) fn schedule_pending_evidence_hashes(
    case_root: &Path,
    case_id: &str,
    app: Option<&AppHandle>,
    task_manager: Arc<TaskManager>,
) -> Result<Vec<String>, CommandError> {
    schedule_pending_evidence_hashes_internal(case_root, case_id, app, task_manager, None, None)
}

fn schedule_pending_evidence_hashes_internal(
    case_root: &Path,
    case_id: &str,
    app: Option<&AppHandle>,
    task_manager: Arc<TaskManager>,
    excluded_task_id: Option<&str>,
    excluded_source_id: Option<&str>,
) -> Result<Vec<String>, CommandError> {
    let db_path = case_root.join("app.db");
    let connection = app_services::connection::open_case_db(&db_path)
        .map_err(CommandError::from_typed_service_error)?;
    let case_id = CaseId(case_id.to_string());
    let sources = list_pending_hash_sources(&connection, &case_id)
        .map_err(CommandError::from_typed_service_error)?;
    if task_manager.running_tasks().iter().any(|task_id| {
        task_id.starts_with("evidence-hash:") && Some(task_id.as_str()) != excluded_task_id
    }) {
        return Ok(Vec::new());
    }
    let mut scheduled = Vec::new();
    for source_id in sources {
        if excluded_source_id == Some(source_id.0.as_str()) {
            continue;
        }
        let task_id = hash_task_id(&source_id);
        if task_manager.is_running(&task_id) {
            continue;
        }
        let Some(job_id) = create_hash_job_if_absent(&connection, &case_id, &source_id)
            .map_err(CommandError::from_typed_service_error)?
        else {
            continue;
        };
        emit_hash_queued(app, &job_id);
        let registration = spawn_hash_task(
            &task_manager,
            task_id,
            &case_id,
            source_id.clone(),
            job_id.clone(),
            db_path.clone(),
            app.cloned(),
            task_manager.clone(),
        );
        if let Err(error) = registration {
            let duplicate = matches!(error, TaskRegistrationError::DuplicateTaskId(_));
            if let Err(settle_error) =
                settle_registration_failure(&connection, &job_id, &source_id, duplicate)
            {
                tracing::warn!(error = %settle_error, "Failed to settle evidence hash registration failure");
            }
            tracing::warn!(error = %error, "Failed to register evidence hash task");
            continue;
        }
        scheduled.push(job_id.0);
        // Hashing is disk-bound. Keep one large evidence source in flight and
        // hand off the next pending source after this task settles.
        break;
    }
    Ok(scheduled)
}

fn spawn_hash_task(
    task_manager: &TaskManager,
    task_id: String,
    case_id: &CaseId,
    data_source_id: DataSourceId,
    job_id: JobId,
    db_path: PathBuf,
    app: Option<AppHandle>,
    scheduler: Arc<TaskManager>,
) -> Result<(), TaskRegistrationError> {
    let cancel_token = Arc::new(AtomicBool::new(false));
    let worker_cancel = Arc::clone(&cancel_token);
    let next_db_path = db_path.clone();
    let next_case_id = case_id.clone();
    let next_app = app.clone();
    let current_task_id = task_id.clone();
    let current_source_id = data_source_id.0.clone();
    let scope = TaskScope::data_source(&case_id.0, &data_source_id.0, &job_id.0);
    task_manager.spawn_scoped_with_stack_size(
        task_id,
        scope,
        cancel_token,
        HASH_TASK_STACK_BYTES,
        move || {
            let result = run_background_evidence_hash(
                db_path,
                data_source_id,
                job_id,
                app.as_ref(),
                worker_cancel,
            )
            .map_err(|error| error.message);
            let next_case_root = next_db_path
                .parent()
                .map(std::path::Path::to_path_buf)
                .unwrap_or_else(|| std::path::PathBuf::from("."));
            if let Err(error) = schedule_pending_evidence_hashes_internal(
                &next_case_root,
                &next_case_id.0,
                next_app.as_ref(),
                scheduler,
                Some(&current_task_id),
                Some(&current_source_id),
            ) {
                tracing::warn!(error = %error.message, "Failed to schedule next evidence hash task");
            }
            result
        },
    )
}

fn emit_hash_queued(app: Option<&AppHandle>, job_id: &JobId) {
    let Some(app) = app else { return };
    event_bridge::emit_job_created(app, &job_id.0, EVIDENCE_HASH_JOB_KIND);
    event_bridge::emit_job_progress(app, &job_id.0, 1, "Evidence hash queued");
}

pub(crate) fn hash_task_id(data_source_id: &DataSourceId) -> String {
    format!("evidence-hash:{}", data_source_id.0)
}
