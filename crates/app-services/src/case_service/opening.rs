use super::{ensure_supported_data_source_platforms, CaseServiceError, Result};
use crate::active_case::ActiveCase;
use domain::CaseMeta;
use persistence_sqlite::{
    open_existing,
    repositories::{
        audit_repo::{AuditAction, AuditRepo},
        case_repo::CaseRepo,
    },
    runner,
};
use rusqlite::Connection;
use std::{fs, path::Path};

pub fn open_case(root: &Path) -> Result<ActiveCase> {
    let active = load_case_workspace(root)?;
    ensure_supported_data_source_platforms(&active)?;
    if active.with_conn(has_legacy_single_db_payload)? {
        return Err(CaseServiceError::InvalidCaseDir(
            "This case uses the legacy single-database storage model; re-import is required for the current development version".to_string(),
        ));
    }
    let _ = active.with_conn(|conn| {
        AuditRepo::new(conn).log_simple(
            Some(&active.meta.id.0),
            &AuditAction::CaseOpen,
            Some(&active.meta.id.0),
        )
    });
    Ok(active)
}

pub(super) fn open_case_for_deletion(root: &Path) -> Result<ActiveCase> {
    load_case_workspace(root)
}

fn load_case_workspace(root: &Path) -> Result<ActiveCase> {
    if !root.exists() {
        return Err(CaseServiceError::NotFound(root.to_path_buf()));
    }
    let case_json_path = root.join("case.json");
    if !case_json_path.exists() {
        return Err(CaseServiceError::InvalidCaseDir(
            "case.json not found".to_string(),
        ));
    }
    let case_json = fs::read_to_string(case_json_path)?;
    let case_from_json: CaseMeta = serde_json::from_str(&case_json)
        .map_err(|error| CaseServiceError::InvalidCaseDir(format!("Invalid case.json: {error}")))?;
    let conn = open_existing(&root.join("app.db"))?;
    runner::run_all(&conn)?;
    let stored = CaseRepo::new(&conn)
        .find_by_id(&case_from_json.id)?
        .ok_or_else(|| CaseServiceError::InvalidCaseDir("Case not in database".to_string()))?;
    Ok(ActiveCase::new(stored, root.to_path_buf(), conn))
}

fn has_legacy_single_db_payload(conn: &Connection) -> persistence_sqlite::DbResult<bool> {
    let file_count: i64 =
        conn.query_row("SELECT COUNT(*) FROM file_entries", [], |row| row.get(0))?;
    let artifact_count: i64 =
        conn.query_row("SELECT COUNT(*) FROM artifacts", [], |row| row.get(0))?;
    let timeline_count: i64 =
        conn.query_row("SELECT COUNT(*) FROM timeline_events", [], |row| row.get(0))?;
    Ok(file_count > 0 || artifact_count > 0 || timeline_count > 0)
}
