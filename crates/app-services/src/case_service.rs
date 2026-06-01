use chrono::Utc;
use domain::{CaseId, CaseMeta};
use persistence_sqlite::{
    open_existing, open_or_create,
    repositories::{case_repo::CaseRepo, audit_repo::{AuditRepo, AuditAction}},
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

pub type Result<T> = std::result::Result<T, CaseServiceError>;

const DIRS: &[&str] = &["evidence", "exports", "reports", "indexes", "cache", "logs"];

/// Windows reserved device names that cannot be used as case names.
const RESERVED_NAMES: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7",
    "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8",
    "LPT9",
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
    // Security: prevent arbitrary directory deletion by requiring the case
    // to be within the designated safe cases root directory.
    infrastructure::config::validate_case_root_is_safe(root)
        .map_err(CaseServiceError::InvalidCaseDir)?;

    if !root.exists() {
        return Err(CaseServiceError::NotFound(root.to_path_buf()));
    }

    let case_json_path = root.join("case.json");
    if !case_json_path.exists() {
        return Err(CaseServiceError::InvalidCaseDir(
            "case.json not found — not a valid case directory".to_string(),
        ));
    }

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
