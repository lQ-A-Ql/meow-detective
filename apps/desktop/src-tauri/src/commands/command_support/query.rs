use super::{
    get_case_read_connection, require_active_case, snapshot_active_case, ActiveCaseSnapshot,
};
use crate::state::{case_database::CaseReadConnection, AppState};
use transport::CommandError;

/// Run a case-scoped read operation with a read-only SQLite connection.
///
/// Query commands must use this boundary even when the underlying service
/// accepts `&rusqlite::Connection`; the wrapper is opened with
/// `SQLITE_OPEN_READ_ONLY` and `PRAGMA query_only=ON`.
pub(crate) async fn run_case_read_command<T, F>(
    app_state: AppState,
    operation: F,
) -> Result<T, CommandError>
where
    T: Send + 'static,
    F: FnOnce(&CaseReadConnection, &ActiveCaseSnapshot) -> Result<T, CommandError> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(move || {
        let active = require_active_case(&app_state)?;
        let connection = get_case_read_connection(&app_state)?;
        operation(&connection, &active)
    })
    .await
    .map_err(CommandError::from_join_error)?
}

pub(crate) async fn run_optional_case_read_command<T, F>(
    app_state: AppState,
    empty: T,
    operation: F,
) -> Result<T, CommandError>
where
    T: Send + 'static,
    F: FnOnce(&CaseReadConnection, &ActiveCaseSnapshot) -> Result<T, CommandError> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(move || {
        if snapshot_active_case(&app_state)?.is_none() {
            return Ok(empty);
        }
        let active = require_active_case(&app_state)?;
        let connection = get_case_read_connection(&app_state)?;
        operation(&connection, &active)
    })
    .await
    .map_err(CommandError::from_join_error)?
}
