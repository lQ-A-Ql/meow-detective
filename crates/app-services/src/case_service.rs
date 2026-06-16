use chrono::Utc;
use domain::{CaseId, CaseMeta};
use persistence_sqlite::{
    open_existing, open_or_create,
    repositories::{
        audit_repo::{AuditAction, AuditRepo},
        case_repo::CaseRepo,
        job_repo::JobRepo,
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
use infrastructure::config::validate_case_root_is_safe;

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

    let stored = CaseRepo::new(&conn)
        .find_by_id(&case_from_json.id)?
        .ok_or_else(|| CaseServiceError::InvalidCaseDir("Case not in database".to_string()))?;

    // 记录审计日志
    let audit = AuditRepo::new(&conn);
    let _ = audit.log_simple(
        Some(&stored.id.0),
        &AuditAction::CaseOpen,
        Some(&stored.id.0),
    );

    Ok(ActiveCase::new(stored, root.to_path_buf(), conn))
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
    if !root.exists() {
        return Err(CaseServiceError::NotFound(root.to_path_buf()));
    }

    validate_case_root_is_safe(root).map_err(CaseServiceError::InvalidCaseDir)?;

    let case_json_path = root.join("case.json");
    if !case_json_path.exists() {
        return Err(CaseServiceError::InvalidCaseDir(
            "case.json not found — not a valid case directory".to_string(),
        ));
    }

    let active = open_case(root)?;
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
    Err(CaseServiceError::Io(last_err.unwrap_or_else(|| {
        std::io::Error::other("Failed to delete case after retries")
    })))
}

pub fn delete_data_source(conn: &Connection, data_source_id: &str) -> Result<()> {
    // Record audit log before deletion
    let audit = persistence_sqlite::repositories::audit_repo::AuditRepo::new(conn);
    let _ = audit.log_simple(
        None,
        &persistence_sqlite::repositories::audit_repo::AuditAction::DataSourceDelete,
        Some(data_source_id),
    );

    let ds_repo = persistence_sqlite::repositories::datasource_repo::DataSourceRepo::new(conn);
    ds_repo
        .delete_cascade(&domain::DataSourceId(data_source_id.to_string()))
        .map_err(CaseServiceError::Db)?;
    Ok(())
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
}
