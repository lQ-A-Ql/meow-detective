use std::path::PathBuf;

use persistence_sqlite::repositories::audit_repo::{AuditAction, AuditRepo};
use transport::CommandError;

use crate::state::AppState;

#[derive(Clone, Debug)]
pub(crate) struct ActiveCaseSnapshot {
    pub case_id: String,
    pub case_root: PathBuf,
    #[cfg(test)]
    pub db_path: PathBuf,
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
        #[cfg(test)]
        db_path: active.db_path(),
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
mod tests {
    use super::*;
    use app_services::case_service;
    use uuid::Uuid;

    #[test]
    fn active_case_snapshot_and_pool_connection_stay_in_sync() {
        let root = std::env::temp_dir().join(format!(
            "Meow_Detective-command-support-test-{}",
            Uuid::new_v4()
        ));
        let active =
            case_service::create_case(&root, "Command Support", Some("Codex Test")).unwrap();
        let db_path = active.db_path();
        let state = AppState::default();

        assert!(snapshot_active_case(&state).unwrap().is_none());

        *state.active_case.lock().unwrap() = Some(active);
        state.init_db_pragmas().unwrap();

        let snapshot = require_active_case(&state).unwrap();
        assert_eq!(snapshot.db_path, db_path);
        assert_eq!(snapshot.case_root.parent(), Some(root.as_path()));
        get_case_connection(&state).unwrap();

        *state.active_case.lock().unwrap() = None;
        let err = require_active_case(&state).unwrap_err();
        assert_eq!(err.code, "NO_ACTIVE_CASE");

        state.clear_db_state().unwrap();
        std::fs::remove_dir_all(root).ok();
    }
}
