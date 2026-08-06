use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use app_services::hash_service::{
    evidence_jobs::{
        create_hash_job_if_absent, list_pending_hash_sources, load_hash_source,
        settle_registration_failure, EVIDENCE_HASH_JOB_KIND,
    },
    EvidenceHashError, HashService,
};
use domain::{CaseId, DataSourceId, JobId};
use tauri::AppHandle;
use transport::CommandError;

use crate::events::event_bridge;
use crate::state::{TaskManager, TaskRegistrationError, TaskScope};

mod progress;
mod status;

use progress::{finish_progress_reporter, spawn_progress_reporter};
use status::{cancel_hash, complete_hash, fail_hash, fail_hash_setup};

pub(crate) fn schedule_pending_evidence_hashes(
    case_root: &Path,
    case_id: &str,
    app: Option<&AppHandle>,
    task_manager: Arc<TaskManager>,
) -> Result<Vec<String>, CommandError> {
    let db_path = case_root.join("app.db");
    let connection = app_services::connection::open_case_db(&db_path)
        .map_err(CommandError::from_typed_service_error)?;
    let case_id = CaseId(case_id.to_string());
    let sources = list_pending_hash_sources(&connection, &case_id)
        .map_err(CommandError::from_typed_service_error)?;
    let mut scheduled = Vec::new();
    for source_id in sources {
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
) -> Result<(), TaskRegistrationError> {
    let cancel_token = Arc::new(AtomicBool::new(false));
    let worker_cancel = Arc::clone(&cancel_token);
    let scope = TaskScope::data_source(&case_id.0, &data_source_id.0, &job_id.0);
    task_manager.spawn_scoped(task_id, scope, cancel_token, move || {
        run_background_evidence_hash(db_path, data_source_id, job_id, app.as_ref(), worker_cancel)
            .map_err(|error| error.message)
    })
}

pub(crate) fn run_background_evidence_hash(
    db_path: PathBuf,
    data_source_id: DataSourceId,
    job_id: JobId,
    app: Option<&AppHandle>,
    cancel_token: Arc<AtomicBool>,
) -> Result<(), CommandError> {
    let connection = app_services::connection::open_case_db(&db_path)
        .map_err(CommandError::from_typed_service_error)?;
    if cancel_token.load(Ordering::Acquire) {
        cancel_hash(&connection, &data_source_id, &job_id, app)?;
        return Ok(());
    }
    if let Some(app) = app {
        event_bridge::emit_job_started(app, &job_id.0, "Evidence hash started");
        event_bridge::emit_job_progress(app, &job_id.0, 2, "Hashing evidence in background");
    }
    let source = match load_hash_source(&connection, &data_source_id) {
        Ok(source) => source,
        Err(error) => return fail_hash_setup(&connection, &data_source_id, &job_id, app, error),
    };
    let reporter = spawn_progress_reporter(&db_path, &data_source_id, &job_id, app);
    let progress_sender = reporter.as_ref().map(progress::ProgressReporter::sender);
    let hash_result = HashService::hash_evidence(
        &source.source_path,
        &source.kind,
        &cancel_token,
        &move |completed, total| {
            if let Some(sender) = &progress_sender {
                let _ = sender.try_send((completed, total));
            }
        },
    );
    finish_progress_reporter(reporter);
    match hash_result {
        Ok(result) => complete_hash(&connection, &data_source_id, &job_id, app, &result),
        Err(EvidenceHashError::Cancelled) if cancel_token.load(Ordering::Acquire) => {
            cancel_hash(&connection, &data_source_id, &job_id, app)
        }
        Err(error) => fail_hash(&connection, &data_source_id, &job_id, app, error),
    }
}

fn emit_hash_queued(app: Option<&AppHandle>, job_id: &JobId) {
    let Some(app) = app else {
        return;
    };
    event_bridge::emit_job_created(app, &job_id.0, EVIDENCE_HASH_JOB_KIND);
    event_bridge::emit_job_progress(app, &job_id.0, 1, "Evidence hash queued");
}

fn hash_task_id(data_source_id: &DataSourceId) -> String {
    format!("evidence-hash:{}", data_source_id.0)
}

#[cfg(test)]
#[path = "../../../../tests/unit/commands/import/evidence_hash.rs"]
mod tests;
