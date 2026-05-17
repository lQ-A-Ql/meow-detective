use tauri::State;
use transport::dto::{ReportHistoryItemDto, ReportTemplateDto};

use crate::state::AppState;

#[tauri::command]
pub fn get_report_templates() -> Result<Vec<ReportTemplateDto>, String> {
    Ok(app_services::report_service::get_report_templates())
}

#[tauri::command]
pub fn get_report_history() -> Result<Vec<ReportHistoryItemDto>, String> {
    Ok(app_services::report_service::get_report_history())
}

#[tauri::command]
pub fn export_html_report(state: State<AppState>) -> Result<String, String> {
    let guard = state.active_case.lock().map_err(|e| e.to_string())?;
    let active = guard.as_ref().ok_or("No active case")?;

    let output_dir = active.case_root.join("reports");
    let case = &active.meta;

    active
        .with_conn(|conn| {
            app_services::report_service::generate_html_report(conn, case, &output_dir)
                .map_err(persistence_sqlite::DbError::System)
        })
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn export_csv_report(state: State<AppState>) -> Result<String, String> {
    let guard = state.active_case.lock().map_err(|e| e.to_string())?;
    let active = guard.as_ref().ok_or("No active case")?;

    let output_dir = active.case_root.join("reports");
    active
        .with_conn(|conn| {
            app_services::report_service::generate_csv_artifacts(conn, &output_dir)
                .map_err(persistence_sqlite::DbError::System)
        })
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn export_json_report(state: State<AppState>) -> Result<String, String> {
    let guard = state.active_case.lock().map_err(|e| e.to_string())?;
    let active = guard.as_ref().ok_or("No active case")?;

    let output_dir = active.case_root.join("reports");
    active
        .with_conn(|conn| {
            app_services::report_service::generate_json_export(conn, &output_dir)
                .map_err(persistence_sqlite::DbError::System)
        })
        .map_err(|e| e.to_string())
}
