use tauri::State;
use transport::{
    commands::ExportScopeDto,
    dto::{ReportHistoryItemDto, ReportTemplateDto},
    CommandError,
};

use crate::state::AppState;

/// Get available report templates (static data, no DB access).
#[tauri::command]
pub fn get_report_templates() -> Result<Vec<ReportTemplateDto>, CommandError> {
    Ok(app_services::report_service::get_report_templates())
}

/// Get report generation history from database.
#[tauri::command]
pub fn get_report_history(
    state: State<'_, AppState>,
) -> Result<Vec<ReportHistoryItemDto>, CommandError> {
    let app_state = state.inner().clone();
    let guard = app_state
        .active_case
        .lock()
        .map_err(|e| CommandError::from_lock_error("Case", e))?;
    let active = guard.as_ref().ok_or_else(CommandError::no_active_case)?;
    let db_path = active.db_path();
    let case_id = active.meta.id.0.clone();
    drop(guard);

    let conn =
        persistence_sqlite::open_existing(&db_path).map_err(CommandError::from_service_error)?;
    Ok(app_services::report_service::get_report_history(
        &conn, &case_id,
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
        // Short lock: extract case info and db_path, then release
        let (db_path, case_root, case_meta) = {
            let guard = app_state
                .active_case
                .lock()
                .map_err(|e| CommandError::from_lock_error("Case", e))?;
            let active = guard.as_ref().ok_or_else(CommandError::no_active_case)?;
            (
                active.db_path(),
                active.case_root.clone(),
                active.meta.clone(),
            )
        };
        // Guard is now dropped — export with released lock
        let output_dir = case_root.join("reports");
        let conn = persistence_sqlite::open_or_create(&db_path)
            .map_err(CommandError::from_service_error)?;
        app_services::report_service::generate_html_report(&conn, &case_meta, &output_dir, &scope)
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
        // Short lock: extract db_path and case_root, then release
        let (db_path, case_root, case_id) = {
            let guard = app_state
                .active_case
                .lock()
                .map_err(|e| CommandError::from_lock_error("Case", e))?;
            let active = guard.as_ref().ok_or_else(CommandError::no_active_case)?;
            (
                active.db_path(),
                active.case_root.clone(),
                active.meta.id.0.clone(),
            )
        };
        // Guard is now dropped — export with released lock
        let output_dir = case_root.join("reports");
        let conn = persistence_sqlite::open_or_create(&db_path)
            .map_err(CommandError::from_service_error)?;
        app_services::report_service::generate_csv_artifacts(&conn, &case_id, &output_dir, &scope)
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
        // Short lock: extract db_path and case_root, then release
        let (db_path, case_root, case_id) = {
            let guard = app_state
                .active_case
                .lock()
                .map_err(|e| CommandError::from_lock_error("Case", e))?;
            let active = guard.as_ref().ok_or_else(CommandError::no_active_case)?;
            (
                active.db_path(),
                active.case_root.clone(),
                active.meta.id.0.clone(),
            )
        };
        // Guard is now dropped — export with released lock
        let output_dir = case_root.join("reports");
        let conn = persistence_sqlite::open_or_create(&db_path)
            .map_err(CommandError::from_service_error)?;
        app_services::report_service::generate_json_export(&conn, &case_id, &output_dir, &scope)
            .map_err(CommandError::from_service_error)
    })
    .await
    .map_err(CommandError::from_join_error)?
}
