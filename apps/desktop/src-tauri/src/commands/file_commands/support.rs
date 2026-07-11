use crate::commands::command_support::{
    get_case_connection, require_active_case, snapshot_active_case, ActiveCaseSnapshot,
};
use crate::state::AppState;
use transport::CommandError;

pub(super) async fn run_active_case_command<T, F>(
    app_state: AppState,
    operation: F,
) -> Result<T, CommandError>
where
    T: Send + 'static,
    F: FnOnce(&rusqlite::Connection, &ActiveCaseSnapshot) -> Result<T, CommandError>
        + Send
        + 'static,
{
    tauri::async_runtime::spawn_blocking(move || {
        let connection = get_case_connection(&app_state)?;
        let active = require_active_case(&app_state)?;
        operation(&connection, &active)
    })
    .await
    .map_err(CommandError::from_join_error)?
}

pub(super) async fn run_optional_active_case_command<T, F>(
    app_state: AppState,
    empty: T,
    operation: F,
) -> Result<T, CommandError>
where
    T: Send + 'static,
    F: FnOnce(&rusqlite::Connection, &ActiveCaseSnapshot) -> Result<T, CommandError>
        + Send
        + 'static,
{
    tauri::async_runtime::spawn_blocking(move || {
        if snapshot_active_case(&app_state)?.is_none() {
            return Ok(empty);
        }
        let connection = get_case_connection(&app_state)?;
        let active = require_active_case(&app_state)?;
        operation(&connection, &active)
    })
    .await
    .map_err(CommandError::from_join_error)?
}

pub(super) fn write_file_extract_audit(
    state: &AppState,
    file_id: &str,
    details: serde_json::Value,
) {
    // Stage 3 compatibility: audit action ownership remains in persistence until
    // the Stage 4 service boundary can expose a file-extraction audit operation.
    crate::commands::command_support::write_audit_log(
        state,
        persistence_sqlite::repositories::audit_repo::AuditAction::FileExtract,
        Some(file_id),
        details,
    );
}
