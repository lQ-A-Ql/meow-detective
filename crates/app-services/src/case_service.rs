use chrono::Utc;
use domain::{CaseId, CaseMeta};
use persistence_sqlite::{
    open_or_create,
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

mod data_source_deletion;
mod opening;
mod platform_compatibility;
pub use data_source_deletion::{delete_data_source, delete_data_source_in};
pub use opening::open_case;
use opening::open_case_for_deletion;
pub use platform_compatibility::ensure_supported_data_source_platforms;

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
    #[error("Unsupported data source platform in case: {0}")]
    UnsupportedPlatform(String),
    #[error("Data source '{data_source_id}' deletion requires recovery from case tombstone '{tombstone}': {reason}")]
    DataSourceDeleteRecoveryPending {
        data_source_id: String,
        tombstone: String,
        reason: String,
    },
    #[error("Data source '{data_source_id}' registration was deleted, but case tombstone cleanup is pending at '{tombstone}': {source}")]
    DataSourceDeleteCleanupPending {
        data_source_id: String,
        tombstone: String,
        #[source]
        source: std::io::Error,
    },
    #[error("Data source '{data_source_id}' deletion failed and rollback step '{step}' also failed at case tombstone '{tombstone}'; original error: {original}; rollback error: {rollback}")]
    DataSourceDeleteRollbackFailed {
        data_source_id: String,
        tombstone: String,
        step: &'static str,
        #[source]
        original: Box<CaseServiceError>,
        rollback: std::io::Error,
    },
}

impl From<crate::source_db::ReadySourceError> for CaseServiceError {
    fn from(error: crate::source_db::ReadySourceError) -> Self {
        match error {
            crate::source_db::ReadySourceError::Db(error) => Self::Db(error),
            crate::source_db::ReadySourceError::UnsupportedPlatform { .. } => {
                Self::UnsupportedPlatform(error.to_string())
            }
            crate::source_db::ReadySourceError::NotFound { .. }
            | crate::source_db::ReadySourceError::NotReady { .. } => {
                Self::InvalidCaseDir(error.to_string())
            }
        }
    }
}

impl transport::ServiceErrorCategory for CaseServiceError {
    fn category(&self) -> transport::ErrorCategory {
        match self {
            Self::Io(_)
            | Self::DataSourceDeleteRecoveryPending { .. }
            | Self::DataSourceDeleteCleanupPending { .. }
            | Self::DataSourceDeleteRollbackFailed { .. } => transport::ErrorCategory::Io,
            Self::Db(_) => transport::ErrorCategory::Io,
            Self::Json(_) => transport::ErrorCategory::Parser,
            Self::AlreadyExists(_) | Self::InvalidCaseDir(_) => {
                transport::ErrorCategory::Validation
            }
            Self::UnsupportedPlatform(_) => transport::ErrorCategory::Unsupported,
            Self::NotFound(_) => transport::ErrorCategory::Validation,
        }
    }

    fn code(&self) -> Option<&'static str> {
        match self {
            Self::DataSourceDeleteRecoveryPending { .. } => {
                Some("DATA_SOURCE_DELETE_RECOVERY_PENDING")
            }
            Self::DataSourceDeleteCleanupPending { .. } => {
                Some("DATA_SOURCE_DELETE_CLEANUP_PENDING")
            }
            Self::DataSourceDeleteRollbackFailed { .. } => {
                Some("DATA_SOURCE_DELETE_ROLLBACK_FAILED")
            }
            _ => None,
        }
    }

    fn user_message(&self) -> Option<&'static str> {
        match self {
            Self::DataSourceDeleteRecoveryPending { .. } => {
                Some("Data source deletion is waiting for managed-storage recovery.")
            }
            Self::DataSourceDeleteCleanupPending { .. } => Some(
                "The data source registration was deleted, but managed-storage cleanup is pending.",
            ),
            Self::DataSourceDeleteRollbackFailed { .. } => {
                Some("Data source deletion failed and rollback requires recovery.")
            }
            _ => None,
        }
    }

    fn recoverable(&self) -> Option<bool> {
        match self {
            Self::DataSourceDeleteRecoveryPending { .. }
            | Self::DataSourceDeleteCleanupPending { .. }
            | Self::DataSourceDeleteRollbackFailed { .. } => Some(true),
            _ => None,
        }
    }

    fn safe_details(&self) -> Option<serde_json::Value> {
        match self {
            Self::DataSourceDeleteRecoveryPending {
                data_source_id,
                tombstone,
                ..
            } => Some(serde_json::json!({
                "dataSourceId": data_source_id,
                "tombstone": tombstone,
                "registrationDeleted": false,
                "state": "recoveryPending"
            })),
            Self::DataSourceDeleteCleanupPending {
                data_source_id,
                tombstone,
                ..
            } => Some(serde_json::json!({
                "dataSourceId": data_source_id,
                "tombstone": tombstone,
                "registrationDeleted": true,
                "state": "cleanupPending"
            })),
            Self::DataSourceDeleteRollbackFailed {
                data_source_id,
                tombstone,
                step,
                ..
            } => Some(serde_json::json!({
                "dataSourceId": data_source_id,
                "tombstone": tombstone,
                "registrationDeleted": false,
                "state": "rollbackFailed",
                "rollbackStep": step
            })),
            _ => None,
        }
    }

    fn suggestion(&self) -> Option<&'static str> {
        match self {
            Self::DataSourceDeleteRecoveryPending { .. }
            | Self::DataSourceDeleteRollbackFailed { .. } => Some(
                "Preserve the tombstone, review backend logs and recovery state, then retry the deletion after recovery.",
            ),
            Self::DataSourceDeleteCleanupPending { .. } => Some(
                "Retry managed-storage cleanup; the data source registration has already been removed.",
            ),
            _ => None,
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
    let active = open_case_for_deletion(root)?;
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
    remove_dir_all_with_retry(root, 5)?;
    Ok(())
}

fn remove_dir_all_with_retry(path: &Path, attempts: usize) -> std::io::Result<()> {
    let mut last_error = None;
    for attempt in 0..attempts {
        if !path.try_exists()? {
            return Ok(());
        }
        match fs::remove_dir_all(path) {
            Ok(()) => return Ok(()),
            Err(error) => last_error = Some(error),
        }
        if attempt + 1 < attempts {
            std::thread::sleep(std::time::Duration::from_millis(200 * (attempt as u64 + 1)));
        }
    }
    Err(last_error.unwrap_or_else(|| std::io::Error::other("directory cleanup was not attempted")))
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
    for (_, source_conn) in
        crate::source_db::open_ready_source_connections(conn, case_root, case_id)?
    {
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
/// still left in `running` or `cancelling` state. Those jobs are marked as
/// `failed` with reason `interrupted_during_close`.
///
/// The `timeout_ms` parameter documents the drain window that was used by the
/// caller; it is recorded in the job detail for diagnostics.
///
/// The database connection is not closed by this function. The caller owns
/// releasing the connection pool afterwards.
pub fn close_case_drain(conn: &Connection, _case_id: &str, timeout_ms: u64) -> Result<DrainResult> {
    let repo = JobRepo::new(conn);
    let interrupted = repo.find_interrupted()?;

    let mut pending_jobs = Vec::with_capacity(interrupted.len());
    let mut warnings = Vec::with_capacity(interrupted.len());

    for job_id in &interrupted {
        let detail = format!("interrupted_during_close (drain timeout {}ms)", timeout_ms);
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
