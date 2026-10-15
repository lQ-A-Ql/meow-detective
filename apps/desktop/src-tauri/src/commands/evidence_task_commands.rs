use crate::state::AppState;
use tauri::{AppHandle, State};
use transport::{
    dto::{EvidenceTaskDto, EvidenceTaskRequestDto},
    CommandError,
};

#[tauri::command]
pub async fn get_content_index_coverage(
    state: State<'_, AppState>,
    data_source_id: String,
) -> Result<transport::dto::ContentIndexCoverageDto, CommandError> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let active = crate::commands::command_support::require_active_case(&state)?;
        let conn = app_services::connection::open_case_db(&active.case_root.join("app.db"))
            .map_err(CommandError::from_typed_service_error)?;
        app_services::content_index_service::coverage::get(
            &conn,
            &active.case_root,
            &domain::CaseId(active.case_id),
            &data_source_id,
        )
    })
    .await
    .map_err(CommandError::from_join_error)?
}
#[tauri::command]
pub async fn start_evidence_task(
    state: State<'_, AppState>,
    app: AppHandle,
    request: EvidenceTaskRequestDto,
) -> Result<EvidenceTaskDto, CommandError> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        crate::evidence_task_jobs::start(state, app, request)
    })
    .await
    .map_err(CommandError::from_join_error)?
}
#[tauri::command]
pub async fn get_evidence_task(
    state: State<'_, AppState>,
    task_id: String,
) -> Result<EvidenceTaskDto, CommandError> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || crate::evidence_task_jobs::get(&state, &task_id))
        .await
        .map_err(CommandError::from_join_error)?
}
#[tauri::command]
pub async fn cancel_evidence_task(
    state: State<'_, AppState>,
    task_id: String,
) -> Result<EvidenceTaskDto, CommandError> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        crate::evidence_task_jobs::cancel(&state, &task_id)
    })
    .await
    .map_err(CommandError::from_join_error)?
}
