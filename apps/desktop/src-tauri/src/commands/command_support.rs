use std::path::PathBuf;

use persistence_sqlite::repositories::audit_repo::{AuditAction, AuditRepo};
use transport::CommandError;

use crate::state::AppState;

#[derive(Clone, Debug)]
pub(crate) struct ActiveCaseSnapshot {
    pub case_id: String,
    pub case_root: PathBuf,
    pub meta: domain::CaseMeta,
}

pub(crate) fn snapshot_active_case(
    state: &AppState,
) -> Result<Option<ActiveCaseSnapshot>, CommandError> {
    let guard = state
        .active_case
        .lock()
        .map_err(|e| CommandError::from_lock_error("Case", e))?;

    Ok(guard.as_ref().map(|active| ActiveCaseSnapshot {
        case_id: active.meta.id.0.clone(),
        case_root: active.case_root.clone(),
        meta: active.meta.clone(),
    }))
}

pub(crate) fn require_active_case(state: &AppState) -> Result<ActiveCaseSnapshot, CommandError> {
    snapshot_active_case(state)?.ok_or_else(CommandError::no_active_case)
}

/// Get a fresh connection to the active case's database.
pub(crate) fn get_case_connection(state: &AppState) -> Result<rusqlite::Connection, CommandError> {
    state.get_connection().map_err(|error| {
        if error.contains("No active case") {
            CommandError::no_active_case()
        } else {
            CommandError::from_service_error(error)
        }
    })
}

/// Get the case ID from the active case (if any).
/// Get the case ID from the active case (if any).
pub fn current_case_id(state: &AppState) -> Option<String> {
    state
        .active_case
        .lock()
        .ok()
        .and_then(|guard| guard.as_ref().map(|active| active.meta.id.0.clone()))
}

/// Write an audit log entry for the active case.
pub fn write_audit_log(
    state: &AppState,
    action: AuditAction,
    resource_id: Option<&str>,
    details: serde_json::Value,
) {
    let case_id = current_case_id(state);
    let details_str = serde_json::to_string(&details).unwrap_or_else(|_| "{}".to_string());
    if let Ok(conn) = state.get_connection() {
        let _ = AuditRepo::new(&conn).log(
            case_id.as_deref(),
            "system",
            &action,
            resource_id,
            &details_str,
        );
    }
}

#[cfg(test)]
#[path = "../../tests/unit/commands/command_support.rs"]
mod tests;
