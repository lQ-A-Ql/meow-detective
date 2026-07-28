use std::path::{Path, PathBuf};

use domain::{CaseId, DataSourceKind};
use persistence_sqlite::repositories::datasource_repo::DataSourceRepo;
use rusqlite::Connection;

use crate::file_service::FileServiceError;

pub(crate) enum DestinationScope<'a> {
    Unscoped,
    ExternalCase {
        case_conn: &'a Connection,
        case_root: &'a Path,
        case_id: &'a CaseId,
    },
    CaseManaged {
        case_conn: &'a Connection,
        case_root: &'a Path,
        case_id: &'a CaseId,
    },
}

pub(crate) fn prepare_destination(
    destination: &Path,
    overwrite: bool,
    scope: DestinationScope<'_>,
) -> Result<PathBuf, FileServiceError> {
    if !destination.is_absolute() {
        return Err(FileServiceError::invalid_input(
            "destinationPath must be an absolute path",
        ));
    }
    if destination.file_name().is_none() {
        return Err(FileServiceError::invalid_input(
            "destinationPath must include a file name",
        ));
    }
    reject_windows_alternate_data_stream(destination)?;
    reject_existing_target(destination, overwrite)?;

    let resolved = resolve_with_missing_tail(destination)?;
    validate_scope(&resolved, &scope)?;
    let parent = resolved.parent().ok_or_else(|| {
        FileServiceError::invalid_input("destinationPath must have a parent directory")
    })?;
    std::fs::create_dir_all(parent)?;

    let canonical_parent = parent.canonicalize()?;
    let target = canonical_parent.join(resolved.file_name().ok_or_else(|| {
        FileServiceError::invalid_input("destinationPath must include a file name")
    })?);
    validate_scope(&target, &scope)?;
    reject_existing_target(&target, overwrite)?;
    Ok(target)
}

fn reject_windows_alternate_data_stream(path: &Path) -> Result<(), FileServiceError> {
    #[cfg(windows)]
    for component in path.components() {
        if matches!(
            component,
            std::path::Component::Prefix(_) | std::path::Component::RootDir
        ) {
            continue;
        }
        if component.as_os_str().to_string_lossy().contains(':') {
            return Err(FileServiceError::security(
                "destinationPath must not use a Windows alternate data stream",
            ));
        }
    }
    Ok(())
}

fn reject_existing_target(path: &Path, overwrite: bool) -> Result<(), FileServiceError> {
    let Ok(metadata) = std::fs::symlink_metadata(path) else {
        return Ok(());
    };
    if metadata.file_type().is_symlink() {
        return Err(FileServiceError::security(
            "destinationPath must not be a symbolic link",
        ));
    }
    if metadata.is_dir() {
        return Err(FileServiceError::invalid_input(
            "destinationPath must point to a file, not a directory",
        ));
    }
    if !overwrite {
        return Err(FileServiceError::invalid_input(
            "destinationPath already exists; set overwrite=true to replace it",
        ));
    }
    Ok(())
}

fn validate_scope(path: &Path, scope: &DestinationScope<'_>) -> Result<(), FileServiceError> {
    let (case_conn, case_root, case_id, require_inside_case) = match scope {
        DestinationScope::Unscoped => return Ok(()),
        DestinationScope::ExternalCase {
            case_conn,
            case_root,
            case_id,
        } => (*case_conn, *case_root, *case_id, false),
        DestinationScope::CaseManaged {
            case_conn,
            case_root,
            case_id,
        } => (*case_conn, *case_root, *case_id, true),
    };
    let resolved_case_root = resolve_with_missing_tail(case_root)?;
    let inside_case = path_is_within(path, &resolved_case_root);
    if require_inside_case && !inside_case {
        return Err(FileServiceError::security(
            "Case-managed extraction destination must remain inside the case workspace",
        ));
    }
    if !require_inside_case && inside_case {
        return Err(FileServiceError::security(
            "Extraction destination must not overlap the case workspace",
        ));
    }

    for source in DataSourceRepo::new(case_conn).find_by_case(case_id)? {
        if !source.source_path.is_absolute() {
            continue;
        }
        let protected = resolve_with_missing_tail(&source.source_path)?;
        let protects_tree = source.kind == DataSourceKind::LogicalDirectory || protected.is_dir();
        if path_eq(path, &protected) || (protects_tree && path_is_within(path, &protected)) {
            return Err(FileServiceError::security(
                "Extraction destination must not overlap a registered evidence source",
            ));
        }
    }
    Ok(())
}

fn resolve_with_missing_tail(path: &Path) -> Result<PathBuf, FileServiceError> {
    let mut existing = path.to_path_buf();
    let mut tail = Vec::new();
    while !existing.exists() {
        let name = existing
            .file_name()
            .ok_or_else(|| FileServiceError::invalid_input("destinationPath cannot be resolved"))?;
        tail.push(name.to_os_string());
        existing = existing
            .parent()
            .ok_or_else(|| FileServiceError::invalid_input("destinationPath cannot be resolved"))?
            .to_path_buf();
    }
    let mut resolved = existing.canonicalize()?;
    for component in tail.iter().rev() {
        resolved.push(component);
    }
    Ok(resolved)
}

fn path_is_within(path: &Path, root: &Path) -> bool {
    let path_components = normalized_components(path);
    let root_components = normalized_components(root);
    path_components.len() >= root_components.len()
        && path_components
            .iter()
            .zip(root_components.iter())
            .all(|(left, right)| left == right)
}

fn path_eq(left: &Path, right: &Path) -> bool {
    normalized_components(left) == normalized_components(right)
}

fn normalized_components(path: &Path) -> Vec<String> {
    path.components()
        .map(|component| {
            let value = component.as_os_str().to_string_lossy();
            if cfg!(windows) {
                value.to_lowercase()
            } else {
                value.into_owned()
            }
        })
        .collect()
}

#[cfg(test)]
#[path = "../../../tests/unit/file_service/extraction/policy.rs"]
mod tests;
