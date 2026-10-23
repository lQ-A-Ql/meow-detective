//! Desktop tasks retain their original case instead of consulting UI state in workers.
mod worker;
use crate::{
    commands::command_support::require_active_case,
    state::{AppState, TaskScope},
};
use app_services::evidence_tasks;
use std::sync::{atomic::AtomicBool, Arc};
use tauri::AppHandle;
use transport::{
    dto::{EvidenceTaskDto, EvidenceTaskRequestDto},
    CommandError,
};

pub(crate) fn start(
    state: AppState,
    app: AppHandle,
    request: EvidenceTaskRequestDto,
) -> Result<EvidenceTaskDto, CommandError> {
    let active = require_active_case(&state)?;
    let case = domain::CaseId(active.case_id.clone());
    let db_path = active.case_root.join("app.db");
    let conn = app_services::connection::open_existing_case_db(&db_path)
        .map_err(CommandError::from_typed_service_error)?;
    let plan = evidence_tasks::plan(&conn, &active.case_root, &case, request)?;
    let task = plan.task.clone();
    if !plan.launch {
        return Ok(task);
    }
    let token = Arc::new(AtomicBool::new(false));
    let worker_token = token.clone();
    let manager = state.task_manager.clone();
    let source_id = match &plan.request {
        EvidenceTaskRequestDto::CarvePng(request) => Some(request.data_source_id.clone()),
        EvidenceTaskRequestDto::CarveZip(request) => Some(request.data_source_id.clone()),
        EvidenceTaskRequestDto::Digest(request) => request.data_source_id.clone().or_else(|| {
            request
                .file_id
                .as_deref()
                .and_then(|id| id.strip_prefix("ds:"))
                .and_then(|id| id.split_once(':'))
                .map(|(id, _)| id.to_owned())
        }),
        EvidenceTaskRequestDto::NestedImport(request) => {
            Some(request.parent_data_source_id.clone())
        }
        EvidenceTaskRequestDto::ContentIndex { data_source_id } => Some(data_source_id.clone()),
        EvidenceTaskRequestDto::RegistrySearch { file_id, .. } => {
            app_services::source_db::parse_source_scoped_id("registry file", file_id)
                .ok()
                .map(|(id, _)| id.0)
        }
    };
    let scope = match source_id {
        Some(source) => TaskScope::data_source(case.0.clone(), source, task.task_id.clone()),
        None => TaskScope::case(case.0.clone(), task.task_id.clone()),
    };
    crate::events::event_bridge::emit_job_created(&app, &task.task_id, &task.kind);
    let context = worker::TaskContext {
        root: active.case_root,
        case,
        db_path,
        app: app.clone(),
        plan,
        registry: state.registry_browser.clone(),
        bitlocker: state.bitlocker_runtime.clone(),
        preview: state.preview_runtime.clone(),
    };
    if let Err(error) = manager.spawn_scoped(task.task_id.clone(), scope, token, move || {
        worker::run(context, worker_token)
    }) {
        let error = CommandError::internal(format!("Task registration failed: {error}"));
        evidence_tasks::finish(
            &conn,
            &domain::CaseId(active.case_id),
            &task.task_id,
            Err(error.clone()),
        )?;
        return Err(error);
    }
    Ok(task)
}
pub(crate) fn cancel(state: &AppState, id: &str) -> Result<EvidenceTaskDto, CommandError> {
    let active = require_active_case(state)?;
    let case = domain::CaseId(active.case_id);
    let conn = app_services::connection::open_existing_case_db(&active.case_root.join("app.db"))
        .map_err(CommandError::from_typed_service_error)?;
    let task = evidence_tasks::get(&conn, &case, id)?;
    if matches!(task.state.as_str(), "queued" | "running" | "cancelling") {
        state.task_manager.cancel(id);
        persistence_sqlite::repositories::evidence_task_repo::EvidenceTaskRepo::new(&conn)
            .request_cancel(&case.0, id)
            .map_err(CommandError::from_service_error)?;
        persistence_sqlite::repositories::job_repo::JobRepo::new(&conn)
            .mark_cancelling(&domain::JobId(id.into()), "Cancellation requested")
            .map_err(CommandError::from_service_error)?;
    }
    evidence_tasks::get(&conn, &case, id)
}
