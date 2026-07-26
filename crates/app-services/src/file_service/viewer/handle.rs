//! File handle opening and host-path resolution for logical directories.

use crate::file_service::viewer::{
    descriptor_file_entry, descriptor_for_file_with_cache, safe_relative_path,
    validate_readable_file_entry, PreviewReadContext, FILE_HANDLE_PREFIX,
};
use crate::file_service::{metadata::lookup::mime_for_entry, FileServiceError};
use domain::FileEntryId;
use persistence_sqlite::repositories::file_repo::FileRepo;
use rusqlite::Connection;
use std::path::PathBuf;
use transport::dto::ViewerHandleDto;

pub fn open_file_handle_real<C>(
    mut context: C,
    file_id: &str,
) -> Result<ViewerHandleDto, FileServiceError>
where
    C: PreviewReadContext,
{
    if context.case_id().is_empty() {
        return open_file_handle_uncached(context.conn(), file_id);
    }

    let descriptor =
        descriptor_for_file_with_cache(&mut context, &FileEntryId(file_id.to_string()))?;

    Ok(ViewerHandleDto {
        handle_id: format!("{FILE_HANDLE_PREFIX}{}", descriptor.file_id),
        size: descriptor.size,
        mime: descriptor.mime,
    })
}

pub(crate) fn open_file_handle_uncached(
    conn: &Connection,
    file_id: &str,
) -> Result<ViewerHandleDto, FileServiceError> {
    let repo = FileRepo::new(conn);
    let entry = repo
        .find_by_id(&FileEntryId(file_id.to_string()))?
        .ok_or_else(|| FileServiceError::not_found("File not found"))?;

    validate_readable_file_entry(conn, &entry)?;

    Ok(ViewerHandleDto {
        handle_id: format!("{FILE_HANDLE_PREFIX}{}", entry.id.0),
        size: entry.size.unwrap_or(0),
        mime: mime_for_entry(&entry),
    })
}

pub fn get_file_path_for_entry<C>(
    mut context: C,
    file_id: &str,
) -> Result<PathBuf, FileServiceError>
where
    C: PreviewReadContext,
{
    if !context.case_id().is_empty() {
        let descriptor =
            descriptor_for_file_with_cache(&mut context, &FileEntryId(file_id.to_string()))?;
        if descriptor.source_kind != "logical_directory" {
            return Err(FileServiceError::other(
                "File path only available for logical directories",
            ));
        }

        let entry = descriptor_file_entry(&descriptor);
        return resolve_logical_file_path(&descriptor.source_path, &entry);
    }

    let repo = FileRepo::new(context.conn());
    let entry = repo
        .find_by_id(&FileEntryId(file_id.to_string()))?
        .ok_or_else(|| FileServiceError::not_found("File not found"))?;
    validate_readable_file_entry(context.conn(), &entry)?;

    let (kind, source_path) = repo
        .find_data_source_location(&entry.data_source_id)?
        .ok_or_else(|| FileServiceError::not_found("Data source not found"))?;

    if kind == "logical_directory" {
        let root = PathBuf::from(&source_path).canonicalize()?;
        let relative_path = safe_relative_path(&entry.path)?;
        Ok(root.join(relative_path))
    } else {
        Err(FileServiceError::other(
            "File path only available for logical directories",
        ))
    }
}

fn resolve_logical_file_path(
    source_path: &str,
    entry: &domain::FileEntry,
) -> Result<PathBuf, FileServiceError> {
    let root = PathBuf::from(source_path).canonicalize()?;
    let relative_path = safe_relative_path(&entry.path)?;
    let full_path = root.join(&relative_path);

    let mut check_path = PathBuf::new();
    for component in full_path.components() {
        check_path.push(component);
        if check_path.is_symlink() {
            return Err(FileServiceError::other(format!(
                "Symlink detected in path at '{}' - rejected for security",
                check_path.display()
            )));
        }
    }

    let canonical = full_path.canonicalize()?;

    if !canonical.starts_with(&root) {
        return Err(FileServiceError::path_traversal(
            "File path escapes data source root",
        ));
    }

    if !canonical.is_file() {
        return Err(FileServiceError::other(
            "File entry does not point to a regular file",
        ));
    }

    Ok(canonical)
}
