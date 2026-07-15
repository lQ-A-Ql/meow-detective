use super::{remove_dir_all_with_retry, CaseServiceError, Result};
use domain::DataSourceId;
use persistence_sqlite::repositories::datasource_repo::{DataSourceRepo, DataSourceStorage};
use rusqlite::Connection;
use std::{
    fs,
    path::{Path, PathBuf},
};

pub fn delete_data_source(conn: &Connection, data_source_id: &str) -> Result<()> {
    let ds_id = DataSourceId(data_source_id.to_string());
    if DataSourceRepo::new(conn).find_storage(&ds_id)?.is_none() {
        return Err(not_registered(data_source_id));
    }
    Err(CaseServiceError::InvalidCaseDir(
        "Data source deletion requires the case root".to_string(),
    ))
}

pub fn delete_data_source_in(
    conn: &Connection,
    case_root: &Path,
    data_source_id: &str,
) -> Result<()> {
    validate_data_source_id(data_source_id)?;
    let ds_id = DataSourceId(data_source_id.to_string());
    let ds_repo = DataSourceRepo::new(conn);
    let canonical_root = fs::canonicalize(case_root)?;
    let pending_tombstone = tombstone_path(case_root, data_source_id);
    let Some(storage) = load_deletion_storage(
        &ds_repo,
        case_root,
        &canonical_root,
        data_source_id,
        &ds_id,
        &pending_tombstone,
    )?
    else {
        return Ok(());
    };

    validate_data_source_storage(&storage, data_source_id)?;
    let existing =
        collect_existing_managed_paths(&ds_repo, case_root, &canonical_root, &storage, &ds_id)?;
    clear_empty_pending_tombstone(
        case_root,
        &canonical_root,
        data_source_id,
        &pending_tombstone,
    )?;
    if existing.is_empty() {
        ds_repo.delete_cascade_with_audit(&ds_id, r#"{"storageDisposition":"notPresent"}"#)?;
        return Ok(());
    }

    // Same-volume staging makes the filesystem phase reversible until DB commit.
    let (tombstone, tombstone_rel) =
        create_data_source_tombstone(case_root, &canonical_root, data_source_id)?;
    let staged = stage_managed_paths(data_source_id, &tombstone, &tombstone_rel, existing)?;
    let audit_details = serde_json::json!({
        "registrationState": "deleted",
        "storageDisposition": "tombstoned",
        "tombstoneRelPath": tombstone_rel,
    })
    .to_string();
    if let Err(db_error) = ds_repo.delete_cascade_with_audit(&ds_id, &audit_details) {
        return Err(rollback_staged_deletion(
            data_source_id,
            &tombstone,
            &tombstone_rel,
            &staged,
            format!("database transaction failed: {db_error}"),
            CaseServiceError::Db(db_error),
        ));
    }
    remove_dir_all_with_retry(&tombstone, 5).map_err(|source| {
        CaseServiceError::DataSourceDeleteCleanupPending {
            data_source_id: data_source_id.to_string(),
            tombstone: tombstone_rel,
            source,
        }
    })
}

fn load_deletion_storage(
    ds_repo: &DataSourceRepo<'_>,
    case_root: &Path,
    canonical_root: &Path,
    data_source_id: &str,
    ds_id: &DataSourceId,
    pending_tombstone: &Path,
) -> Result<Option<DataSourceStorage>> {
    let Some(storage) = ds_repo.find_storage(ds_id)? else {
        if !validate_existing_managed_dir(
            case_root,
            canonical_root,
            pending_tombstone,
            "tombstone",
        )? {
            return Err(not_registered(data_source_id));
        }
        remove_dir_all_with_retry(pending_tombstone, 5).map_err(|source| {
            CaseServiceError::DataSourceDeleteCleanupPending {
                data_source_id: data_source_id.to_string(),
                tombstone: tombstone_relative_path(data_source_id),
                source,
            }
        })?;
        return Ok(None);
    };
    Ok(Some(storage))
}

fn collect_existing_managed_paths(
    ds_repo: &DataSourceRepo<'_>,
    case_root: &Path,
    canonical_root: &Path,
    storage: &DataSourceStorage,
    data_source_id: &DataSourceId,
) -> Result<Vec<(&'static str, PathBuf)>> {
    let canonical_evidence = match ds_repo.source_kind(data_source_id)? {
        domain::DataSourceKind::CephRbd => None,
        _ => {
            let evidence_path = PathBuf::from(ds_repo.source_path(data_source_id)?);
            evidence_path
                .try_exists()?
                .then(|| fs::canonicalize(&evidence_path))
                .transpose()?
        }
    };
    let candidates = [
        (
            "source",
            managed_source_dir(case_root, storage, data_source_id)?,
        ),
        ("staging", managed_staging_dir(case_root, storage)?),
    ];
    let mut existing = Vec::with_capacity(candidates.len());
    for (label, path) in candidates {
        if !validate_existing_managed_dir(case_root, canonical_root, &path, label)? {
            continue;
        }
        let canonical_managed = fs::canonicalize(&path)?;
        if canonical_evidence.as_ref().is_some_and(|evidence| {
            canonical_managed.starts_with(evidence) || evidence.starts_with(&canonical_managed)
        }) {
            return Err(CaseServiceError::InvalidCaseDir(
                "Managed deletion path overlaps the original evidence source".to_string(),
            ));
        }
        existing.push((label, path));
    }
    Ok(existing)
}

fn stage_managed_paths(
    data_source_id: &str,
    tombstone_path: &Path,
    tombstone_rel: &str,
    existing: Vec<(&'static str, PathBuf)>,
) -> Result<Vec<StagedDataSourcePath>> {
    let mut staged = Vec::with_capacity(existing.len());
    for (label, original) in existing {
        let tombstone = tombstone_path.join(label);
        if let Err(stage_error) = fs::rename(&original, &tombstone) {
            return Err(rollback_staged_deletion(
                data_source_id,
                tombstone_path,
                tombstone_rel,
                &staged,
                format!("filesystem staging failed: {stage_error}"),
                CaseServiceError::Io(stage_error),
            ));
        }
        staged.push(StagedDataSourcePath {
            label,
            original,
            tombstone,
        });
    }
    Ok(staged)
}

#[derive(Debug)]
struct StagedDataSourcePath {
    label: &'static str,
    original: PathBuf,
    tombstone: PathBuf,
}

fn validate_data_source_storage(storage: &DataSourceStorage, data_source_id: &str) -> Result<()> {
    validate_data_source_id(data_source_id)?;
    let paths = [
        (
            storage.source_db_rel_path.as_deref(),
            Path::new("sources").join(data_source_id).join("source.db"),
        ),
        (
            storage.index_rel_path.as_deref(),
            Path::new("sources").join(data_source_id).join("index"),
        ),
        (
            storage.staging_rel_path.as_deref(),
            Path::new("staging").join(data_source_id),
        ),
    ];
    if storage.storage_model != "source_db"
        || paths
            .iter()
            .any(|(actual, expected)| actual.is_none_or(|path| Path::new(path) != expected))
    {
        return Err(CaseServiceError::InvalidCaseDir(format!(
            "Data source '{data_source_id}' has an invalid managed-storage layout"
        )));
    }
    Ok(())
}

fn validate_data_source_id(data_source_id: &str) -> Result<()> {
    if data_source_id.is_empty()
        || !data_source_id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_'))
    {
        return Err(CaseServiceError::InvalidCaseDir(
            "Data source ID contains unsafe path characters".to_string(),
        ));
    }
    Ok(())
}

fn managed_source_dir(
    case_root: &Path,
    storage: &DataSourceStorage,
    data_source_id: &DataSourceId,
) -> Result<PathBuf> {
    storage
        .source_db_rel_path
        .as_deref()
        .and_then(|path| Path::new(path).parent())
        .map(|path| crate::source_db::safe_case_relative_path(case_root, &path.to_string_lossy()))
        .transpose()
        .map_err(CaseServiceError::Db)
        .map(|path| path.unwrap_or_else(|| crate::source_db::source_dir(case_root, data_source_id)))
}

fn managed_staging_dir(case_root: &Path, storage: &DataSourceStorage) -> Result<PathBuf> {
    let relative = storage.staging_rel_path.as_deref().ok_or_else(|| {
        CaseServiceError::InvalidCaseDir("Data source staging path is missing".to_string())
    })?;
    crate::source_db::safe_case_relative_path(case_root, relative).map_err(CaseServiceError::Db)
}

fn validate_existing_managed_dir(
    case_root: &Path,
    canonical_root: &Path,
    path: &Path,
    label: &str,
) -> Result<bool> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(CaseServiceError::Io(error)),
    };
    let relative = path.strip_prefix(case_root).map_err(|_| {
        CaseServiceError::InvalidCaseDir(format!("Managed {label} path escapes the case"))
    })?;
    let canonical_path = fs::canonicalize(path)?;
    if canonical_path != canonical_root.join(relative) || !metadata.is_dir() {
        return Err(CaseServiceError::InvalidCaseDir(format!(
            "Managed {label} path is not a direct case directory"
        )));
    }
    Ok(true)
}

fn create_data_source_tombstone(
    case_root: &Path,
    canonical_root: &Path,
    data_source_id: &str,
) -> Result<(PathBuf, String)> {
    let cache = case_root.join("cache");
    if !validate_existing_managed_dir(case_root, canonical_root, &cache, "cache")? {
        return Err(CaseServiceError::InvalidCaseDir(
            "Case cache directory is missing".to_string(),
        ));
    }
    let parent = cache.join("data-source-tombstones");
    if !validate_existing_managed_dir(case_root, canonical_root, &parent, "tombstone")? {
        fs::create_dir(&parent)?;
    }
    validate_existing_managed_dir(case_root, canonical_root, &parent, "tombstone")?;
    let tombstone = fs::canonicalize(&parent)?.join(data_source_id);
    fs::create_dir(&tombstone)?;
    Ok((tombstone, tombstone_relative_path(data_source_id)))
}

fn clear_empty_pending_tombstone(
    case_root: &Path,
    canonical_root: &Path,
    data_source_id: &str,
    tombstone: &Path,
) -> Result<()> {
    if !validate_existing_managed_dir(case_root, canonical_root, tombstone, "tombstone")? {
        return Ok(());
    }
    if fs::read_dir(tombstone)?.next().transpose()?.is_some() {
        return Err(CaseServiceError::DataSourceDeleteRecoveryPending {
            data_source_id: data_source_id.to_string(),
            tombstone: tombstone_relative_path(data_source_id),
            reason: "a previous deletion attempt left staged data".to_string(),
        });
    }
    fs::remove_dir(tombstone).map_err(|error| CaseServiceError::DataSourceDeleteRecoveryPending {
        data_source_id: data_source_id.to_string(),
        tombstone: tombstone_relative_path(data_source_id),
        reason: format!("empty tombstone cleanup failed: {error}"),
    })
}

fn rollback_staged_deletion(
    data_source_id: &str,
    tombstone_path: &Path,
    tombstone_rel: &str,
    staged: &[StagedDataSourcePath],
    reason: String,
    original_error: CaseServiceError,
) -> CaseServiceError {
    if let Err(restore_error) = restore_staged_paths(staged) {
        return CaseServiceError::DataSourceDeleteRollbackFailed {
            data_source_id: data_source_id.to_string(),
            tombstone: tombstone_rel.to_string(),
            step: "restoreManagedPaths",
            original: Box::new(original_error),
            rollback: std::io::Error::new(
                restore_error.kind(),
                format!("{reason}; restore failed: {restore_error}"),
            ),
        };
    }
    if let Err(cleanup_error) = fs::remove_dir(tombstone_path) {
        if cleanup_error.kind() != std::io::ErrorKind::NotFound {
            return CaseServiceError::DataSourceDeleteRollbackFailed {
                data_source_id: data_source_id.to_string(),
                tombstone: tombstone_rel.to_string(),
                step: "cleanupEmptyTombstone",
                original: Box::new(original_error),
                rollback: std::io::Error::new(
                    cleanup_error.kind(),
                    format!(
                        "{reason}; managed paths were restored, but tombstone cleanup failed: {cleanup_error}"
                    ),
                ),
            };
        }
    }
    original_error
}

fn restore_staged_paths(staged: &[StagedDataSourcePath]) -> std::io::Result<()> {
    let mut first_error = None;
    for path in staged.iter().rev() {
        let result = path.original.try_exists().and_then(|exists| {
            if exists {
                Err(std::io::Error::new(
                    std::io::ErrorKind::AlreadyExists,
                    "original path was recreated during rollback",
                ))
            } else {
                fs::rename(&path.tombstone, &path.original)
            }
        });
        if let Err(error) = result {
            first_error.get_or_insert_with(|| {
                std::io::Error::new(error.kind(), format!("{}: {error}", path.label))
            });
        }
    }
    first_error.map_or(Ok(()), Err)
}

fn not_registered(data_source_id: &str) -> CaseServiceError {
    CaseServiceError::InvalidCaseDir(format!(
        "Data source '{}' is not registered",
        data_source_id
    ))
}

fn tombstone_path(case_root: &Path, data_source_id: &str) -> PathBuf {
    case_root
        .join("cache")
        .join("data-source-tombstones")
        .join(data_source_id)
}

fn tombstone_relative_path(data_source_id: &str) -> String {
    format!("cache/data-source-tombstones/{data_source_id}")
}

#[cfg(test)]
#[path = "../../tests/unit/case_service/data_source_deletion_test.rs"]
mod tests;
