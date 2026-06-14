use tauri::State;
use transport::{
    commands::ExportScopeDto,
    dto::{ReportHistoryItemDto, ReportTemplateDto},
    CommandError,
};

use super::command_support::{get_case_connection, require_active_case};
use crate::state::AppState;

/// Get available report templates (static data, no DB access).
#[tauri::command]
pub fn get_report_templates() -> Result<Vec<ReportTemplateDto>, CommandError> {
    Ok(app_services::report::get_report_templates())
}

/// Get report generation history from database.
#[tauri::command]
pub fn get_report_history(
    state: State<'_, AppState>,
) -> Result<Vec<ReportHistoryItemDto>, CommandError> {
    let app_state = state.inner().clone();
    let active = require_active_case(&app_state)?;
    let conn = get_case_connection(&app_state)?;
    Ok(app_services::report::get_report_history(
        &conn,
        &active.case_id,
    ))
}

/// Export HTML report for the current case.
#[tauri::command]
pub async fn export_html_report(
    state: State<'_, AppState>,
    scope: Option<ExportScopeDto>,
) -> Result<String, CommandError> {
    let scope = scope.unwrap_or_default();
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let active = require_active_case(&app_state)?;
        let output_dir = active.case_root.join("reports");
        let conn = get_case_connection(&app_state)?;
        app_services::report::generate_html_report(&conn, &active.meta, &output_dir, &scope)
            .map_err(CommandError::from_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}

/// Export CSV report for artifacts.
#[tauri::command]
pub async fn export_csv_report(
    state: State<'_, AppState>,
    scope: Option<ExportScopeDto>,
) -> Result<String, CommandError> {
    let scope = scope.unwrap_or_default();
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let active = require_active_case(&app_state)?;
        let output_dir = active.case_root.join("reports");
        let conn = get_case_connection(&app_state)?;
        app_services::report::generate_csv_artifacts(&conn, &active.case_id, &output_dir, &scope)
            .map_err(CommandError::from_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}

/// Export CSV correlation report.
#[tauri::command]
pub async fn export_csv_correlation_report(
    state: State<'_, AppState>,
    scope: Option<ExportScopeDto>,
) -> Result<String, CommandError> {
    let scope = scope.unwrap_or_default();
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let active = require_active_case(&app_state)?;
        let output_dir = active.case_root.join("reports");
        let conn = get_case_connection(&app_state)?;
        app_services::report::generate_csv_correlation(&conn, &active.case_id, &output_dir, &scope)
            .map_err(CommandError::from_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}

/// Export JSON report.
#[tauri::command]
pub async fn export_json_report(
    state: State<'_, AppState>,
    scope: Option<ExportScopeDto>,
) -> Result<String, CommandError> {
    let scope = scope.unwrap_or_default();
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let active = require_active_case(&app_state)?;
        let output_dir = active.case_root.join("reports");
        let conn = get_case_connection(&app_state)?;
        app_services::report::generate_json_export(&conn, &active.case_id, &output_dir, &scope)
            .map_err(CommandError::from_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}
