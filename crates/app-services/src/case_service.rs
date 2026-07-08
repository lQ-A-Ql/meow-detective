use chrono::Utc;
use domain::{CaseId, CaseMeta, DataSourceId};
use persistence_sqlite::{
    open_existing, open_or_create,
    repositories::{
        artifact_repo::ArtifactRepo,
        audit_repo::{AuditAction, AuditRepo},
        case_repo::{CaseMetrics, CaseRepo},
        datasource_repo::DataSourceRepo,
        file_repo::FileRepo,
        job_repo::JobRepo,
        timeline_repo::TimelineRepo,
    },
    runner,
};
use rusqlite::Connection;
use std::{
    fs,
    path::{Path, PathBuf},
};
use thiserror::Error;
use uuid::Uuid;

use crate::active_case::ActiveCase;

#[derive(Debug, Error)]
pub enum CaseServiceError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Database error: {0}")]
    Db(#[from] persistence_sqlite::DbError),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Case already exists at path: {0}")]
    AlreadyExists(PathBuf),
    #[error("No case found at path: {0}")]
    NotFound(PathBuf),
    #[error("Invalid case directory: {0}")]
    InvalidCaseDir(String),
}

impl transport::ServiceErrorCategory for CaseServiceError {
    fn category(&self) -> transport::ErrorCategory {
        match self {
            Self::Io(_) => transport::ErrorCategory::Io,
            Self::Db(_) => transport::ErrorCategory::Io,
            Self::Json(_) => transport::ErrorCategory::Parser,
            Self::AlreadyExists(_) | Self::InvalidCaseDir(_) => {
                transport::ErrorCategory::Validation
            }
            Self::NotFound(_) => transport::ErrorCategory::Validation,
        }
    }
}

pub type Result<T> = std::result::Result<T, CaseServiceError>;

const DIRS: &[&str] = &["evidence", "exports", "reports", "indexes", "cache", "logs"];

/// Windows reserved device names that cannot be used as case names.
const RESERVED_NAMES: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// Validate a case name to prevent path traversal and injection.
///
/// Rules: alphanumeric, spaces, hyphens, underscores only. Length 1-MAX_CASE_NAME_LENGTH.
/// Rejects path separators, `..`, null bytes, and Windows reserved names.
fn validate_case_name(name: &str) -> Result<()> {
    if name.is_empty() || name.len() > infrastructure::constants::MAX_CASE_NAME_LENGTH {
        return Err(CaseServiceError::InvalidCaseDir(format!(
            "Case name must be 1-{} characters",
            infrastructure::constants::MAX_CASE_NAME_LENGTH
        )));
    }
    if name.contains("..") || name.contains('/') || name.contains('\\') || name.contains('\0') {
        return Err(CaseServiceError::InvalidCaseDir(
            "Case name contains invalid characters (path separators or traversal)".to_string(),
        ));
    }
    if !name
        .chars()
        .all(|c| c.is_alphanumeric() || c == ' ' || c == '-' || c == '_')
    {
        return Err(CaseServiceError::InvalidCaseDir(
            "Case name may only contain letters, digits, spaces, hyphens, and underscores"
                .to_string(),
        ));
    }
    // Check for Windows reserved names
    let upper = name.to_uppercase();
    // split() always returns at least one element, so unwrap_or("") is safe but kept for clarity
    let name_part = upper.split(' ').next().unwrap_or("");
    if RESERVED_NAMES.contains(&name_part) {
        return Err(CaseServiceError::InvalidCaseDir(format!(
            "'{}' is a reserved system name",
            name_part
        )));
    }
    Ok(())
}

/// Create a new forensic case at the given root directory.
///
/// Creates the case directory structure (evidence, exports, reports, indexes, cache, logs),
/// initializes the SQLite database, and writes `case.json` metadata.
/// Returns an `ActiveCase` with an open database connection.
///
/// # Errors
/// Returns `AlreadyExists` if the case directory already exists.
pub fn create_case(root: &Path, name: &str, examiner: Option<&str>) -> Result<ActiveCase> {
    validate_case_name(name)?;
    let case_root = root.join(name);
    if case_root.exists() {
        return Err(CaseServiceError::AlreadyExists(case_root));
    }

    for dir in DIRS {
        fs::create_dir_all(case_root.join(dir))?;
    }

    let db_path = case_root.join("app.db");
    let conn = open_or_create(&db_path)?;
    runner::run_all(&conn)?;

    let now = Utc::now();
    let case = CaseMeta {
        id: CaseId(Uuid::new_v4().to_string()),
        name: name.to_string(),
        number: None,
        examiner: examiner.map(|s| s.to_string()),
        notes: None,
        created_at: now,
        updated_at: now,
    };

    CaseRepo::new(&conn).create(&case)?;

    // 记录审计日志
    let audit = AuditRepo::new(&conn);
    let _ = audit.log(
        Some(&case.id.0),
        "system",
        &AuditAction::CaseCreate,
        Some(&case.id.0),
        &serde_json::json!({"name": name, "examiner": examiner}).to_string(),
    );

    let case_json = serde_json::to_string_pretty(&case)?;
    fs::write(case_root.join("case.json"), case_json)?;

    Ok(ActiveCase::new(case, case_root, conn))
}

/// Open an existing forensic case from the given root directory.
///
/// Reads `case.json` metadata and opens the SQLite database.
/// Validates that the case exists in the database.
///
/// # Errors
/// Returns `NotFound` if the directory doesn't exist, or `InvalidCaseDir`
/// if the directory structure is invalid.
pub fn open_case(root: &Path) -> Result<ActiveCase> {
    if !root.exists() {
        return Err(CaseServiceError::NotFound(root.to_path_buf()));
    }

    let case_json_path = root.join("case.json");
    if !case_json_path.exists() {
        return Err(CaseServiceError::InvalidCaseDir(
            "case.json not found".to_string(),
        ));
    }

    let case_json = fs::read_to_string(&case_json_path)?;
    let case_from_json: CaseMeta = serde_json::from_str(&case_json)
        .map_err(|e| CaseServiceError::InvalidCaseDir(format!("Invalid case.json: {}", e)))?;

    let db_path = root.join("app.db");
    let conn = open_existing(&db_path)?;
    runner::run_all(&conn)?;

    let stored = CaseRepo::new(&conn)
        .find_by_id(&case_from_json.id)?
        .ok_or_else(|| CaseServiceError::InvalidCaseDir("Case not in database".to_string()))?;

    reject_legacy_single_db_case(&conn)?;

    // 记录审计日志
    let audit = AuditRepo::new(&conn);
    let _ = audit.log_simple(
        Some(&stored.id.0),
        &AuditAction::CaseOpen,
        Some(&stored.id.0),
    );

    Ok(ActiveCase::new(stored, root.to_path_buf(), conn))
}

fn reject_legacy_single_db_case(conn: &Connection) -> Result<()> {
    let app_file_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM file_entries", [], |row| row.get(0))
        .map_err(persistence_sqlite::DbError::from)?;
    let app_artifact_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM artifacts", [], |row| row.get(0))
        .map_err(persistence_sqlite::DbError::from)?;
    let app_timeline_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM timeline_events", [], |row| row.get(0))
        .map_err(persistence_sqlite::DbError::from)?;

    if app_file_count > 0 || app_artifact_count > 0 || app_timeline_count > 0 {
        return Err(CaseServiceError::InvalidCaseDir(
            "This case uses the legacy single-database storage model; re-import is required for the current development version".to_string(),
        ));
    }

    Ok(())
}

/// Delete a forensic case directory and all its contents.
///
/// Retries up to 5 times on Windows where SQLite WAL/SHM files
/// may still be held by another process.
///
/// # Errors
/// Returns `NotFound` if the directory doesn't exist, or `InvalidCaseDir`
/// if `case.json` is missing.
pub fn delete_case(root: &Path) -> Result<()> {
    delete_case_in(root)
}

/// Delete a case directory after validating it is a real case workspace.
pub fn delete_case_in(root: &Path) -> Result<()> {
    if !root.exists() {
        return Err(CaseServiceError::NotFound(root.to_path_buf()));
    }

    let case_json_path = root.join("case.json");
    if !case_json_path.exists() {
        return Err(CaseServiceError::InvalidCaseDir(
            "case.json not found — not a valid case directory".to_string(),
        ));
    }

    let active = open_case(root)?;
    let delete_details = serde_json::json!({
        "case_id": active.meta.id.0,
        "case_root": root.display().to_string(),
    })
    .to_string();
    let _ = active.with_conn(|conn| {
        AuditRepo::new(conn).log(
            Some(&active.meta.id.0),
            "system",
            &AuditAction::CaseDelete,
            Some(&active.meta.id.0),
            &delete_details,
        )
    });
    drop(active);

    // Retry removal on Windows where SQLite WAL/SHM files may still be held
    let mut last_err = None;
    for attempt in 0..5 {
        match fs::remove_dir_all(root) {
            Ok(()) => return Ok(()),
            Err(e) => {
                last_err = Some(e);
                if attempt < 4 {
                    std::thread::sleep(std::time::Duration::from_millis(
                        200 * (attempt as u64 + 1),
                    ));
                }
            }
        }
    }
    // After 5 attempts, last_err is guaranteed to be Some
    Err(CaseServiceError::Io(
        last_err.expect("last_err must be Some after retry loop"),
    ))
}

pub fn delete_data_source(conn: &Connection, data_source_id: &str) -> Result<()> {
    // Record audit log before deletion
    let audit = persistence_sqlite::repositories::audit_repo::AuditRepo::new(conn);
    let _ = audit.log_simple(
        None,
        &persistence_sqlite::repositories::audit_repo::AuditAction::DataSourceDelete,
        Some(data_source_id),
    );

    let ds_id = DataSourceId(data_source_id.to_string());
    let ds_repo = DataSourceRepo::new(conn);
    ds_repo
        .delete_cascade(&ds_id)
        .map_err(CaseServiceError::Db)?;
    Ok(())
}

pub fn delete_data_source_in(
    conn: &Connection,
    case_root: &Path,
    data_source_id: &str,
) -> Result<()> {
    let audit = AuditRepo::new(conn);
    let _ = audit.log_simple(None, &AuditAction::DataSourceDelete, Some(data_source_id));

    let ds_id = DataSourceId(data_source_id.to_string());
    let ds_repo = DataSourceRepo::new(conn);
    let storage = ds_repo
        .find_storage(&ds_id)
        .map_err(CaseServiceError::Db)?
        .ok_or_else(|| {
            CaseServiceError::InvalidCaseDir(format!(
                "Data source '{}' is not registered",
                data_source_id
            ))
        })?;

    let source_dir = storage
        .source_db_rel_path
        .as_deref()
        .and_then(|rel| Path::new(rel).parent())
        .map(|rel| crate::source_db::safe_case_relative_path(case_root, &rel.to_string_lossy()))
        .transpose()
        .map_err(CaseServiceError::Db)?
        .unwrap_or_else(|| crate::source_db::source_dir(case_root, &ds_id));

    if source_dir.exists() {
        fs::remove_dir_all(&source_dir)?;
    }
    if let Some(staging_rel_path) = storage.staging_rel_path {
        let staging_path = crate::source_db::safe_case_relative_path(case_root, &staging_rel_path)
            .map_err(CaseServiceError::Db)?;
        if staging_path.exists() {
            fs::remove_dir_all(staging_path)?;
        }
    }

    ds_repo
        .delete_cascade(&ds_id)
        .map_err(CaseServiceError::Db)?;
    Ok(())
}

pub fn get_case_metrics_for_case(
    conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
) -> Result<CaseMetrics> {
    let sources = DataSourceRepo::new(conn).find_by_case(case_id)?;
    let mut metrics = CaseMetrics {
        data_source_count: sources.len() as u64,
        indexed_file_count: 0,
        timeline_event_count: 0,
        artifact_count: 0,
    };

    for source in sources {
        let storage = DataSourceRepo::new(conn).find_storage(&source.id)?;
        if storage
            .as_ref()
            .is_some_and(|value| value.import_state == "failed")
        {
            continue;
        }

        let source_conn =
            match crate::source_db::open_registered_source_db(conn, case_root, &source.id) {
                Ok(source_conn) => source_conn,
                Err(error) => {
                    tracing::warn!(
                        data_source_id = %source.id.0,
                        error = %error,
                        "Skipping source database while building case metrics"
                    );
                    continue;
                }
            };
        metrics.indexed_file_count = metrics
            .indexed_file_count
            .saturating_add(FileRepo::new(&source_conn).count_all()?);
        metrics.timeline_event_count = metrics
            .timeline_event_count
            .saturating_add(TimelineRepo::new(&source_conn).count()?);
        metrics.artifact_count = metrics
            .artifact_count
            .saturating_add(ArtifactRepo::new(&source_conn).count()?);
    }

    Ok(metrics)
}

/// Result of draining running jobs during case close.
#[derive(Debug, Clone)]
pub struct DrainResult {
    /// Whether all jobs drained completely within the timeout.
    pub fully_drained: bool,
    /// IDs of jobs that were still pending and had to be marked as interrupted.
    pub pending_jobs: Vec<String>,
    /// Human-readable warnings about jobs that did not drain in time.
    pub warnings: Vec<String>,
}

/// Finalise database job state during case close.
///
/// After the caller has cancelled all background tasks via `TaskManager` and
/// waited for them to stop, this function checks the database for any jobs
/// still left in `running` or `cancelling` state.  Those jobs are marked as
/// `failed` with reason `interrupted_during_close`.
///
/// The `timeout_ms` parameter documents the drain window that was used by the
/// caller; it is recorded in the job detail for diagnostics.
///
/// The database connection is NOT closed by this function — the caller is
/// responsible for releasing the connection pool afterwards.
pub fn close_case_drain(conn: &Connection, _case_id: &str, timeout_ms: u64) -> Result<DrainResult> {
    let repo = JobRepo::new(conn);
    let interrupted = repo.find_interrupted()?;

    let mut pending_jobs = Vec::with_capacity(interrupted.len());
    let mut warnings = Vec::with_capacity(interrupted.len());

    for job_id in &interrupted {
        let detail = format!("interrupted_during_close (drain timeout {}ms)", timeout_ms);
        // Best-effort: if the fail update itself fails, we still record the warning.
        match repo.fail(job_id, &detail) {
            Ok(()) => {
                warnings.push(format!(
                    "Job {} was still running after {}ms drain timeout — marked as failed",
                    job_id.0, timeout_ms
                ));
            }
            Err(e) => {
                warnings.push(format!(
                    "Job {} still running after {}ms drain timeout, but failed to mark as interrupted: {}",
                    job_id.0, timeout_ms, e
                ));
            }
        }
        pending_jobs.push(job_id.0.clone());
    }

    Ok(DrainResult {
        fully_drained: interrupted.is_empty(),
        pending_jobs,
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use persistence_sqlite::repositories::job_repo::JobRepo;

    fn setup_db() -> (rusqlite::Connection, String) {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        persistence_sqlite::runner::run_all(&conn).unwrap();
        let case_id = "test-case-1";
        conn.execute(
            "INSERT INTO cases (id, name, number, examiner) VALUES (?1, 'Test', '1', 'qa')",
            rusqlite::params![case_id],
        )
        .unwrap();
        (conn, case_id.to_string())
    }

    #[test]
    fn no_running_jobs_drains_immediately() {
        let (conn, case_id) = setup_db();
        // No jobs at all — drain should report fully_drained
        let result = close_case_drain(&conn, &case_id, 5000).unwrap();
        assert!(result.fully_drained);
        assert!(result.pending_jobs.is_empty());
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn drain_timeout_marks_jobs_interrupted() {
        let (conn, case_id) = setup_db();
        let repo = JobRepo::new(&conn);

        // Create a running job (simulates a job that didn't stop in time)
        let running_id = repo.create(&case_id, "import").unwrap();

        // Create a cancelling job (simulates a job mid-cancellation)
        let cancelling_id = repo.create(&case_id, "import").unwrap();
        repo.mark_cancelling(&cancelling_id, "Cancel requested")
            .unwrap();

        // Create a completed job (should be untouched)
        let completed_id = repo.create(&case_id, "index").unwrap();
        repo.complete(&completed_id, "done").unwrap();

        let result = close_case_drain(&conn, &case_id, 5000).unwrap();

        // Both running + cancelling should be drained
        assert!(!result.fully_drained);
        assert_eq!(result.pending_jobs.len(), 2);
        assert!(result.pending_jobs.contains(&running_id.0));
        assert!(result.pending_jobs.contains(&cancelling_id.0));
        assert_eq!(result.warnings.len(), 2);

        // Verify DB state: running + cancelling jobs are now 'failed'
        let jobs = persistence_sqlite::repositories::job_repo::JobRepo::new(&conn)
            .list_recent(10)
            .unwrap();

        let running_snapshot = jobs.iter().find(|j| j.id.0 == running_id.0).unwrap();
        assert_eq!(running_snapshot.status, "failed");
        assert!(running_snapshot.detail.contains("interrupted_during_close"));

        let cancelling_snapshot = jobs.iter().find(|j| j.id.0 == cancelling_id.0).unwrap();
        assert_eq!(cancelling_snapshot.status, "failed");
        assert!(cancelling_snapshot
            .detail
            .contains("interrupted_during_close"));

        // Completed job unchanged
        let completed_snapshot = jobs.iter().find(|j| j.id.0 == completed_id.0).unwrap();
        assert_eq!(completed_snapshot.status, "completed");
    }

    #[test]
    fn drain_completes_when_jobs_finish_quickly() {
        let (conn, case_id) = setup_db();
        let repo = JobRepo::new(&conn);

        // Create a job and then complete it (simulates a job that finished before drain)
        let job_id = repo.create(&case_id, "quick-task").unwrap();
        repo.complete(&job_id, "finished quickly").unwrap();

        let result = close_case_drain(&conn, &case_id, 5000).unwrap();
        assert!(result.fully_drained);
        assert!(result.pending_jobs.is_empty());
        assert!(result.warnings.is_empty());

        // Job should still be 'completed', not 'failed'
        let jobs = persistence_sqlite::repositories::job_repo::JobRepo::new(&conn)
            .list_recent(10)
            .unwrap();
        let snapshot = jobs.iter().find(|j| j.id.0 == job_id.0).unwrap();
        assert_eq!(snapshot.status, "completed");
    }

    #[test]
    fn open_case_rejects_legacy_single_database_payloads() {
        let tmp = tempfile::TempDir::new().unwrap();
        let active = create_case(tmp.path(), "legacy_case", Some("tester")).unwrap();
        let case_root = active.case_root.clone();

        active
            .with_conn(|conn| {
                conn.execute(
                    "INSERT INTO data_sources
                     (id, case_id, name, kind, source_path, storage_model)
                     VALUES ('legacy-ds', ?1, 'Legacy source', 'logical_directory', 'D:/legacy', 'source_db')",
                    [&active.meta.id.0],
                )?;
                conn.execute(
                    "INSERT INTO file_entries
                     (id, parent_id, data_source_id, path, name, entry_type, size, deleted, hidden, system)
                     VALUES ('legacy-file', NULL, 'legacy-ds', '/', '/', 'directory', NULL, 0, 0, 0)",
                    [],
                )?;
                Ok(())
            })
            .unwrap();
        drop(active);

        let error = match open_case(&case_root) {
            Ok(_) => panic!("legacy app.db payload should be rejected"),
            Err(error) => error,
        };

        assert!(error.to_string().contains("legacy single-database"));
        assert!(error.to_string().contains("re-import is required"));
    }
}
