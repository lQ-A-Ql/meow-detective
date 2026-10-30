use std::{
    path::Path,
    sync::{atomic::AtomicBool, mpsc, Arc},
};

use persistence_sqlite::repositories::job_repo::JobRepo;
use tauri::AppHandle;
use transport::CommandError;

use crate::{
    events::event_bridge,
    state::{AppState, TaskRegistrationError, TaskScope},
};

pub(crate) struct AnalysisTaskRequest<'a> {
    pub(crate) case_id: &'a str,
    pub(crate) case_root: &'a Path,
    pub(crate) data_source_id: Option<&'a str>,
    pub(crate) kind: &'a str,
}

/// Register a long-running analysis writer with the case lifecycle manager and
/// preserve its terminal state in the durable job table.
pub(crate) fn run<T, F>(
    state: &AppState,
    app: &AppHandle,
    request: AnalysisTaskRequest<'_>,
    operation: F,
) -> Result<T, CommandError>
where
    T: Send + 'static,
    F: FnOnce(&rusqlite::Connection, Arc<AtomicBool>) -> Result<T, CommandError> + Send + 'static,
{
    let db_path = request.case_root.join("app.db");
    let connection = app_services::connection::open_existing_case_db(&db_path)
        .map_err(CommandError::from_typed_service_error)?;
    let job_repo = JobRepo::new(&connection);
    let job_id = job_repo
        .create(request.case_id, request.kind)
        .map_err(CommandError::from_typed_service_error)?;
    job_repo
        .update_progress(&job_id, 1, "Queued for analysis admission")
        .map_err(CommandError::from_typed_service_error)?;
    drop(connection);

    let task_id = job_id.0.clone();
    let scope = match request.data_source_id {
        Some(data_source_id) => {
            TaskScope::data_source(request.case_id, data_source_id, task_id.clone())
        }
        None => TaskScope::case(request.case_id, task_id.clone()),
    };
    let token = Arc::new(AtomicBool::new(false));
    let worker_token = Arc::clone(&token);
    let (result_tx, result_rx) = mpsc::sync_channel(1);
    let worker_app = app.clone();
    let worker_kind = request.kind.to_string();
    let worker_db_path = db_path.clone();
    let worker_job_id = task_id.clone();
    let manager = state.task_manager.clone();
    let registration = manager.spawn_scoped(task_id.clone(), scope, token, move || {
        let result = execute(
            Some(&worker_app),
            &worker_db_path,
            &worker_job_id,
            &worker_kind,
            worker_token,
            operation,
        );
        result_tx
            .send(result)
            .map_err(|_| "Analysis task result receiver closed".to_string())
    });
    if let Err(error) = registration {
        settle_registration_failure(&db_path, &job_id, &error)?;
        return Err(registration_error(error));
    }

    event_bridge::emit_job_created(app, &task_id, request.kind);
    let result = result_rx
        .recv()
        .map_err(|_| CommandError::internal("Analysis task ended without a result"))??;
    crate::case_projection_jobs::schedule(
        &state.task_manager,
        app,
        request.case_id,
        request.case_root,
    )?;
    Ok(result)
}

pub(crate) fn start_evidence_classification(
    state: &AppState,
    app: &AppHandle,
    request: AnalysisTaskRequest<'_>,
    categories: Vec<String>,
    runtime: app_services::analysis_service::AnalysisSourceReadRuntime,
) -> Result<String, CommandError> {
    let db_path = request.case_root.join("app.db");
    let connection = app_services::connection::open_existing_case_db(&db_path)
        .map_err(CommandError::from_typed_service_error)?;
    let job_id = JobRepo::new(&connection)
        .create(request.case_id, request.kind)
        .map_err(CommandError::from_typed_service_error)?;
    JobRepo::new(&connection)
        .update_progress(&job_id, 1, "Queued for analysis admission")
        .map_err(CommandError::from_typed_service_error)?;
    drop(connection);

    let task_id = job_id.0.clone();
    let scope = request
        .data_source_id
        .map(|source| TaskScope::data_source(request.case_id, source, task_id.clone()))
        .unwrap_or_else(|| TaskScope::case(request.case_id, task_id.clone()));
    let token = Arc::new(AtomicBool::new(false));
    let worker_token = Arc::clone(&token);
    let worker_app = app.clone();
    let worker_state = state.clone();
    let worker_db_path = db_path.clone();
    let worker_job_id = task_id.clone();
    let worker_kind = request.kind.to_string();
    let worker_case_id = request.case_id.to_string();
    let worker_case_root = request.case_root.to_path_buf();
    let worker_source_id = request.data_source_id.map(str::to_owned);
    let schedule_case_id = worker_case_id.clone();
    let event_kind = request.kind.to_string();
    let registration = state
        .task_manager
        .spawn_scoped(task_id.clone(), scope, token, move || {
            let Some(source_id) = worker_source_id else {
                return Err("analysis requires a data source".into());
            };
            let source_runtime = runtime;
            let operation_case_root = worker_case_root.clone();
            let result = execute(
                Some(&worker_app),
                &worker_db_path,
                &worker_job_id,
                &worker_kind,
                worker_token,
                move |case_conn, cancel_token| {
                    let category_refs = categories.iter().map(String::as_str).collect::<Vec<_>>();
                    app_services::analysis_service::run_source_evidence_scan_with_cancel(
                        case_conn,
                        &operation_case_root,
                        &domain::CaseId(worker_case_id),
                        &domain::DataSourceId(source_id),
                        &category_refs,
                        &source_runtime,
                        &cancel_token,
                    )
                    .map_err(CommandError::from_typed_service_error)
                },
            );
            if result.is_ok() {
                let _ = crate::case_projection_jobs::schedule(
                    &worker_state.task_manager,
                    &worker_app,
                    &schedule_case_id,
                    &worker_case_root,
                );
            }
            result.map(|_| ()).map_err(|error| error.to_string())
        });
    if let Err(error) = registration {
        settle_registration_failure(&db_path, &job_id, &error)?;
        return Err(registration_error(error));
    }
    event_bridge::emit_job_created(app, &task_id, &event_kind);
    Ok(task_id)
}

fn execute<T, F>(
    app: Option<&AppHandle>,
    db_path: &Path,
    job_id: &str,
    kind: &str,
    cancel_token: Arc<AtomicBool>,
    operation: F,
) -> Result<T, CommandError>
where
    T: Send + 'static,
    F: FnOnce(&rusqlite::Connection, Arc<AtomicBool>) -> Result<T, CommandError> + Send + 'static,
{
    let connection = app_services::connection::open_existing_case_db(db_path)
        .map_err(CommandError::from_typed_service_error)?;
    let job = domain::JobId(job_id.to_string());
    let repo = JobRepo::new(&connection);
    if !repo
        .start(&job)
        .map_err(CommandError::from_typed_service_error)?
    {
        return Err(CommandError::cancelled(
            "Analysis task cancelled before execution",
        ));
    }
    repo.update_progress(&job, 2, "Analysis task admitted")
        .map_err(CommandError::from_typed_service_error)?;
    if let Some(app) = app {
        event_bridge::emit_job_started(app, job_id, &format!("{kind} started"));
        event_bridge::emit_job_progress(app, job_id, 2, "Analysis task admitted");
    }

    match operation(&connection, cancel_token) {
        Ok(value) => {
            let detail = format!("{kind} completed");
            repo.complete_committed(&job, &detail)
                .map_err(CommandError::from_typed_service_error)?;
            emit_completed(app, job_id, &detail);
            Ok(value)
        }
        Err(error) if error.category == "cancelled" => {
            let detail = error.message.clone();
            repo.cancel(&job, &detail)
                .map_err(CommandError::from_typed_service_error)?;
            if let Some(app) = app {
                event_bridge::emit_job_cancelled(app, job_id, &detail);
            }
            Err(error)
        }
        Err(error) => {
            let detail = error.message.clone();
            repo.fail(&job, &detail)
                .map_err(CommandError::from_typed_service_error)?;
            if let Some(app) = app {
                event_bridge::emit_job_failed(app, job_id, &detail);
            }
            Err(error)
        }
    }
}

fn emit_completed(app: Option<&AppHandle>, job_id: &str, detail: &str) {
    if let Some(app) = app {
        event_bridge::emit_job_progress(app, job_id, 100, detail);
        event_bridge::emit_job_completed(app, job_id, detail);
    }
}

fn settle_registration_failure(
    db_path: &Path,
    job_id: &domain::JobId,
    error: &TaskRegistrationError,
) -> Result<(), CommandError> {
    let connection = app_services::connection::open_existing_case_db(db_path)
        .map_err(CommandError::from_typed_service_error)?;
    let detail = format!("Analysis task registration failed: {error}");
    JobRepo::new(&connection)
        .fail(job_id, &detail)
        .map_err(CommandError::from_typed_service_error)
}

fn registration_error(error: TaskRegistrationError) -> CommandError {
    match error {
        TaskRegistrationError::DuplicateTaskId(_) => {
            CommandError::conflict("Analysis task is already running")
        }
        TaskRegistrationError::RetiredCase(_) | TaskRegistrationError::RetiredDataSource { .. } => {
            CommandError::conflict("Analysis task targets a retired case or data source")
        }
        other => CommandError::internal(format!("Analysis task registration failed: {other}")),
    }
}

#[cfg(test)]
#[path = "../tests/unit/analysis_task_jobs.rs"]
mod tests;
