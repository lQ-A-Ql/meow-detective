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
    let result = state
        .get_connection()
        .map_err(CommandError::from_service_error)
        .and_then(|connection| {
            let case_id = crate::commands::command_support::current_case_id(state);
            persist_file_extract_audit(&connection, case_id.as_deref(), file_id, details)
        });
    if let Err(error) = result {
        tracing::error!(
            file_id,
            error_code = %error.code,
            "Failed to persist file extraction audit record"
        );
    }
}

pub(super) fn persist_file_extract_audit(
    connection: &rusqlite::Connection,
    case_id: Option<&str>,
    file_id: &str,
    details: serde_json::Value,
) -> Result<(), CommandError> {
    app_services::file_service::record_file_extraction_audit(connection, case_id, file_id, &details)
        .map_err(CommandError::from_typed_service_error)
}
