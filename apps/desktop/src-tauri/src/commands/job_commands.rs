use transport::dto::{JobSnapshotDto, TraceItemDto, WarningItemDto};

#[tauri::command]
pub fn get_jobs_snapshot() -> Result<Vec<JobSnapshotDto>, String> {
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
