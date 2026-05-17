use transport::dto::{ReportHistoryItemDto, ReportTemplateDto};

#[tauri::command]
pub fn get_report_templates() -> Result<Vec<ReportTemplateDto>, String> {
    Ok(app_services::report_service::get_report_templates())
}

#[tauri::command]
pub fn get_report_history() -> Result<Vec<ReportHistoryItemDto>, String> {
    Ok(app_services::report_service::get_report_history())
}
