use tauri::State;
use transport::dto::{JobSnapshotDto, TraceItemDto, WarningItemDto};

use crate::state::AppState;

#[tauri::command]
pub fn get_jobs_snapshot(state: State<AppState>) -> Result<Vec<JobSnapshotDto>, String> {
    let guard = state.active_case.lock().map_err(|e| e.to_string())?;
    let Some(active) = guard.as_ref() else {
        return Ok(vec![]);
    };
    active
        .with_conn(|conn| {
            app_services::job_service::get_jobs_from_db(conn)
                .map_err(persistence_sqlite::DbError::System)
        })
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_warnings(state: State<AppState>) -> Result<Vec<WarningItemDto>, String> {
    let guard = state.active_case.lock().map_err(|e| e.to_string())?;
    if guard.is_none() {
        return Ok(vec![]);
    }
    Ok(vec![])
}

#[tauri::command]
pub fn get_trace_items(state: State<AppState>) -> Result<Vec<TraceItemDto>, String> {
    let guard = state.active_case.lock().map_err(|e| e.to_string())?;
    if guard.is_none() {
        return Ok(vec![]);
    }
    Ok(vec![])
}
