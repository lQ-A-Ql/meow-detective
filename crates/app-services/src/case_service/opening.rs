use super::{
    platform_compatibility::ensure_supported_data_source_platforms_for_case, CaseServiceError,
    Result,
};
use crate::active_case::ActiveCase;
use domain::{CaseId, CaseMeta};
use persistence_sqlite::{
    open_existing,
    repositories::{
        audit_repo::{AuditAction, AuditRepo},
        case_repo::CaseRepo,
    },
    runner,
};
use rusqlite::{Connection, OpenFlags};
use std::{fs, path::Path};

pub fn open_case(root: &Path) -> Result<ActiveCase> {
    let (case_from_json, db_path) = validate_case_workspace(root)?;
    let stored = preflight_case_workspace(&db_path, &case_from_json.id)?;
    let active = ActiveCase::new(stored, root.to_path_buf(), open_existing(&db_path)?);
    active.with_conn(|conn| {
        crate::source_db::migrate_ready_source_databases(conn, root, &active.meta.id)
    })?;
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
    let (case_from_json, db_path) = validate_case_workspace(root)?;
    let conn = open_existing(&db_path)?;
    let stored = load_case_record(&conn, &case_from_json.id)?;
    Ok(ActiveCase::new(stored, root.to_path_buf(), conn))
}

fn validate_case_workspace(root: &Path) -> Result<(CaseMeta, std::path::PathBuf)> {
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
    Ok((case_from_json, root.join("app.db")))
}

fn preflight_case_workspace(db_path: &Path, case_id: &CaseId) -> Result<CaseMeta> {
    let conn = Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(persistence_sqlite::DbError::from)?;
    ensure_current_schema(&conn)?;
    let stored = load_case_record(&conn, case_id)?;
    ensure_supported_data_source_platforms_for_case(&conn, case_id)?;
    if has_legacy_single_db_payload(&conn)? {
        return Err(CaseServiceError::InvalidCaseDir(
            "This case uses the legacy single-database storage model; re-import is required for the current development version".to_string(),
        ));
    }
    Ok(stored)
}

fn ensure_current_schema(conn: &Connection) -> Result<()> {
    let current = runner::current_version(conn)?;
    if current.as_deref() != Some(runner::latest_version()) {
        return Err(CaseServiceError::InvalidCaseDir(format!(
            "Case schema is incompatible with this development version (found {}, expected {}); re-import is required",
            current.as_deref().unwrap_or("unversioned"),
            runner::latest_version()
        )));
    }
    Ok(())
}

fn load_case_record(conn: &Connection, case_id: &CaseId) -> Result<CaseMeta> {
    CaseRepo::new(conn)
        .find_by_id(case_id)?
        .ok_or_else(|| CaseServiceError::InvalidCaseDir("Case not in database".to_string()))
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
