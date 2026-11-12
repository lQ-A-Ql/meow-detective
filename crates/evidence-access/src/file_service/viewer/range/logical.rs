use std::io::Read;

use domain::FileEntry;

use crate::file_service::{
    viewer::{descriptor_file_entry, safe_relative_path, PreviewDescriptor, RangeContentReader},
    FileServiceError,
};

pub(super) fn open_descriptor_file(
    descriptor: &PreviewDescriptor,
) -> Result<RangeContentReader, FileServiceError> {
    let entry = descriptor_file_entry(descriptor);
    open_file_seekable(&descriptor.source_path, &entry).map(RangeContentReader::Seekable)
}

pub(super) fn open_descriptor_seekable(
    descriptor: &PreviewDescriptor,
) -> Result<Box<dyn evidence_core::ReadSeek>, FileServiceError> {
    open_file_seekable(&descriptor.source_path, &descriptor_file_entry(descriptor))
}

pub(super) fn open_file(
    source_path: &str,
    entry: &FileEntry,
) -> Result<Box<dyn Read>, FileServiceError> {
    Ok(Box::new(std::fs::File::open(resolve_path(
        source_path,
        entry,
    )?)?))
}

pub(super) fn open_file_seekable(
    source_path: &str,
    entry: &FileEntry,
) -> Result<Box<dyn evidence_core::ReadSeek>, FileServiceError> {
    Ok(Box::new(std::fs::File::open(resolve_path(
        source_path,
        entry,
    )?)?))
}

fn resolve_path(
    source_path: &str,
    entry: &FileEntry,
) -> Result<std::path::PathBuf, FileServiceError> {
    let root = std::path::PathBuf::from(source_path).canonicalize()?;
    let relative_path = safe_relative_path(&entry.path)?;
    let full_path = root.join(relative_path);
    reject_symlink_components(&full_path)?;
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

fn reject_symlink_components(path: &std::path::Path) -> Result<(), FileServiceError> {
    let mut current = std::path::PathBuf::new();
    for component in path.components() {
        current.push(component);
        if current.is_symlink() {
            return Err(FileServiceError::other(
                "Symlink detected in logical source path - rejected for security",
            ));
        }
    }
    Ok(())
}
