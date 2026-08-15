use super::{opening::open_case_for_deletion, CaseServiceError, Result};
use crate::active_case::ActiveCase;
use chrono::Utc;
use domain::{CaseId, CaseMeta};
use persistence_sqlite::{
    open_or_create,
    repositories::{
        audit_repo::{AuditAction, AuditRepo},
        case_repo::CaseRepo,
    },
    runner,
};
use std::{fs, path::Path};
use uuid::Uuid;

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

pub(super) fn remove_dir_all_with_retry(path: &Path, attempts: usize) -> std::io::Result<()> {
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
