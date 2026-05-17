use tauri::State;
use transport::dto::{JobSnapshotDto, TraceItemDto, WarningItemDto};

use crate::state::AppState;

#[tauri::command]
pub fn get_jobs_snapshot(state: State<AppState>) -> Result<Vec<JobSnapshotDto>, String> {
    let guard = state.active_case.lock().map_err(|e| e.to_string())?;
    if let Some(active) = guard.as_ref() {
        let jobs = active
            .with_conn(|conn| {
                app_services::job_service::get_jobs_from_db(conn)
                    .map_err(persistence_sqlite::DbError::System)
            })
            .map_err(|e| e.to_string())?;
        if !jobs.is_empty() {
            return Ok(jobs);
        }
    }
    Ok(app_services::job_service::get_jobs_snapshot())
}

#[tauri::command]
pub fn get_warnings() -> Result<Vec<WarningItemDto>, String> {
    Ok(app_services::job_service::get_warnings())
}

#[tauri::command]
pub fn get_trace_items() -> Result<Vec<TraceItemDto>, String> {
    Ok(app_services::job_service::get_trace_items())
}
