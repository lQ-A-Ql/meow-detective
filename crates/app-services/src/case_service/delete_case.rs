use super::{opening::open_case_for_deletion, CaseServiceError, Result};
use persistence_sqlite::repositories::audit_repo::{AuditAction, AuditRepo};
use std::{fs, path::Path};

pub fn delete_case(root: &Path) -> Result<()> {
    delete_case_in(root)
}

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
