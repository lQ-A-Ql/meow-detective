use chrono::Utc;
use domain::{CaseId, CaseMeta};
use persistence_sqlite::{open_or_create, repositories::case_repo::CaseRepo, runner};
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

pub fn create_case(root: &Path, name: &str, examiner: Option<&str>) -> Result<ActiveCase> {
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

    let case_json = serde_json::to_string_pretty(&case)?;
    fs::write(case_root.join("case.json"), case_json)?;

    Ok(ActiveCase::new(case, case_root, conn))
}

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
    let conn = open_or_create(&db_path)?;

    let stored = CaseRepo::new(&conn)
        .find_by_id(&case_from_json.id)?
        .ok_or_else(|| CaseServiceError::InvalidCaseDir("Case not in database".to_string()))?;

    Ok(ActiveCase::new(stored, root.to_path_buf(), conn))
}
