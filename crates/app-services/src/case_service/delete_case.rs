use super::{opening::open_case_for_deletion, CaseServiceError, Result};
use persistence_sqlite::repositories::{
    audit_repo::{AuditAction, AuditRepo},
    datasource_repo::DataSourceRepo,
};
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
    let source_ids = active.with_conn(|connection| {
        DataSourceRepo::new(connection)
            .find_by_case(&active.meta.id)
            .map(|sources| {
                sources
                    .into_iter()
                    .map(|source| source.id.0)
                    .collect::<Vec<_>>()
            })
    })?;
    let case_id = active.meta.id.0.clone();
    drop(active);
    crate::search_service::clear_case_file_search_cache(&case_id)
        .map_err(|error| CaseServiceError::Io(std::io::Error::other(error)))?;
    let _source_store_guard = crate::storage_runtime::close_case_source_stores(root)
        .map_err(|error| CaseServiceError::InvalidCaseDir(error.to_string()))?
        .ok_or_else(|| {
            CaseServiceError::StorageHandlesInUse(
                source_ids
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "case".to_owned()),
            )
        })?;
    let delete_details = serde_json::json!({
        "case_id": case_id,
        "case_root": root.display().to_string(),
    })
    .to_string();
    if let Ok(connection) = persistence_sqlite::open_existing(&root.join("app.db")) {
        let _ = AuditRepo::new(&connection).log(
            Some(&case_id),
            "system",
            &AuditAction::CaseDelete,
            Some(&case_id),
            &delete_details,
        );
    }
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
