use tauri::State;
use transport::{
    dto::{JobSnapshotDto, TraceItemDto, WarningItemDto},
    CommandError,
};

use crate::state::AppState;

#[tauri::command]
pub async fn get_jobs_snapshot(
    state: State<'_, AppState>,
) -> Result<Vec<JobSnapshotDto>, CommandError> {
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let guard = app_state
            .active_case
            .lock()
            .map_err(|e| CommandError::from_lock_error("Case", e))?;
        let Some(active) = guard.as_ref() else {
            return Ok(vec![]);
        };
        active
            .with_conn(|conn| {
                app_services::job_service::get_jobs_from_db(conn)
                    .map_err(persistence_sqlite::DbError::System)
            })
            .map_err(CommandError::from_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}

#[tauri::command]
pub async fn get_warnings(state: State<'_, AppState>) -> Result<Vec<WarningItemDto>, CommandError> {
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let guard = app_state
            .active_case
            .lock()
            .map_err(|e| CommandError::from_lock_error("Case", e))?;
        if guard.is_none() {
            return Ok(vec![]);
        }
        Ok(vec![])
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
        let guard = app_state
            .active_case
            .lock()
            .map_err(|e| CommandError::from_lock_error("Case", e))?;
        if guard.is_none() {
            return Ok(vec![]);
        }
        Ok(vec![])
    })
    .await
    .map_err(CommandError::from_join_error)?
}
