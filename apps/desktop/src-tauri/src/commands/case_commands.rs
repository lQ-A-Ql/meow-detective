use transport::dto::{CaseMetricsDto, CaseSummaryDto, RecentObjectDto};

#[tauri::command]
pub fn get_current_case() -> Result<CaseSummaryDto, String> {
    Ok(app_services::case_service::get_current_case())
}

#[tauri::command]
pub fn get_case_metrics() -> Result<CaseMetricsDto, String> {
    Ok(app_services::case_service::get_case_metrics())
}

#[tauri::command]
pub fn get_recent_objects() -> Result<Vec<RecentObjectDto>, String> {
    Ok(app_services::case_service::get_recent_objects())
}
