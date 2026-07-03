use tauri::State;
use transport::{
    dto::{JobSnapshotDto, TraceItemDto, WarningItemDto},
    CommandError,
};

use super::command_support::{get_case_connection, snapshot_active_case};
use crate::state::AppState;

#[tauri::command]
pub async fn get_jobs_snapshot(
    state: State<'_, AppState>,
) -> Result<Vec<JobSnapshotDto>, CommandError> {
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        if snapshot_active_case(&app_state)?.is_none() {
            return Ok(vec![]);
        }
        let conn = get_case_connection(&app_state)?;
        app_services::job_service::get_jobs_from_db(&conn)
            .map_err(CommandError::from_typed_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}

#[tauri::command]
pub async fn get_warnings(state: State<'_, AppState>) -> Result<Vec<WarningItemDto>, CommandError> {
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        if snapshot_active_case(&app_state)?.is_none() {
            return Ok(vec![]);
        }
        let conn = get_case_connection(&app_state)?;
        app_services::job_service::get_warnings_from_db(&conn)
            .map_err(CommandError::from_typed_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}

#[tauri::command]
pub async fn get_trace_items(
    state: State<'_, AppState>,
) -> Result<Vec<TraceItemDto>, CommandError> {
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        if snapshot_active_case(&app_state)?.is_none() {
            return Ok(vec![]);
        }
        let conn = get_case_connection(&app_state)?;
        app_services::job_service::get_trace_items_from_db(&conn)
            .map_err(CommandError::from_typed_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}
