use crate::state::AppState;
use tauri::{AppHandle, State};
use transport::{
    dto::{EvidenceTaskDto, EvidenceTaskRequestDto},
    CommandError,
};

use super::command_support::run_case_read_command;

#[tauri::command]
pub async fn get_content_index_coverage(
    state: State<'_, AppState>,
    data_source_id: String,
) -> Result<transport::dto::ContentIndexCoverageDto, CommandError> {
    let app_state = state.inner().clone();
    run_case_read_command(app_state, move |conn, active| {
        app_services::content_index_service::coverage::get(
            conn,
            &active.case_root,
            &active.meta.id,
            &data_source_id,
        )
    })
    .await
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
    let app_state = state.inner().clone();
    run_case_read_command(app_state, move |conn, active| {
        app_services::evidence_tasks::get(conn, &active.meta.id, &task_id)
    })
    .await
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
