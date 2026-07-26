use std::io::Read;

use domain::FileEntry;
use persistence_sqlite::repositories::file_repo::FileRepo;
use rusqlite::Connection;

use crate::file_service::{
    viewer::{
        descriptor_file_entry, open_descriptor_image_file, open_descriptor_image_file_with_context,
        open_e01_file, open_e01_reader_cached, open_raw_file, resolve_partition_index_for_entry,
        validate_readable_file_entry, PreviewDescriptor, PreviewReadContext, RangeContentReader,
    },
    FileServiceError,
};

pub(crate) fn open_file_content_for_descriptor(
    descriptor: &PreviewDescriptor,
) -> Result<Box<dyn Read>, FileServiceError> {
    let reader = match descriptor.source_kind.as_str() {
        "logical_directory" => open_logical_descriptor_file(descriptor),
        "e01" => open_e01_descriptor_file(descriptor),
        "raw" => open_raw_descriptor_file(descriptor),
        other => unsupported_source(other),
    }?;
    Ok(match reader {
        RangeContentReader::Seekable(reader) => reader as Box<dyn Read>,
        RangeContentReader::Streaming(reader) => reader,
    })
}

pub(crate) fn open_file_content_for_descriptor_with_context<C>(
    context: &mut C,
    descriptor: &PreviewDescriptor,
) -> Result<Box<dyn Read>, FileServiceError>
where
    C: PreviewReadContext,
{
    if descriptor.source_kind != "ceph_rbd" {
        return open_file_content_for_descriptor(descriptor);
    }
    Ok(
        match open_descriptor_image_file_with_context(context, descriptor)? {
            RangeContentReader::Seekable(reader) => reader as Box<dyn Read>,
            RangeContentReader::Streaming(reader) => reader,
        },
    )
}

fn open_logical_descriptor_file(
    descriptor: &PreviewDescriptor,
) -> Result<RangeContentReader, FileServiceError> {
    let entry = descriptor_file_entry(descriptor);
    open_logical_file_seekable(&descriptor.source_path, &entry).map(RangeContentReader::Seekable)
}

fn open_logical_descriptor_seekable(
    descriptor: &PreviewDescriptor,
) -> Result<Box<dyn evidence_core::ReadSeek>, FileServiceError> {
    open_logical_file_seekable(&descriptor.source_path, &descriptor_file_entry(descriptor))
}

pub(crate) fn open_range_content_for_descriptor(
    descriptor: &PreviewDescriptor,
) -> Result<RangeContentReader, FileServiceError> {
    match descriptor.source_kind.as_str() {
        "logical_directory" => {
            open_logical_descriptor_seekable(descriptor).map(RangeContentReader::Seekable)
        }
        "e01" => open_e01_descriptor_file(descriptor),
        "raw" => open_raw_descriptor_file(descriptor),
        other => unsupported_source(other),
    }
}

pub(crate) fn open_range_content_for_descriptor_with_context<C>(
    context: &mut C,
    descriptor: &PreviewDescriptor,
) -> Result<RangeContentReader, FileServiceError>
where
    C: PreviewReadContext,
{
    if descriptor.source_kind != "ceph_rbd" {
        return open_range_content_for_descriptor(descriptor);
    }
    open_descriptor_image_file_with_context(context, descriptor)
}

fn open_e01_descriptor_file(
    descriptor: &PreviewDescriptor,
) -> Result<RangeContentReader, FileServiceError> {
    let case_id = descriptor.case_id.clone();
    open_descriptor_image_file(descriptor, move |source_path| {
        open_e01_reader_cached(source_path, &case_id)
            .map(|reader| Box::new(reader) as Box<dyn evidence_core::EvidenceReader>)
    })
}

fn open_raw_descriptor_file(
    descriptor: &PreviewDescriptor,
) -> Result<RangeContentReader, FileServiceError> {
    open_descriptor_image_file(descriptor, |source_path| {
        evidence_core::RawImageReader::open(source_path)
            .map(|reader| Box::new(reader) as Box<dyn evidence_core::EvidenceReader>)
    })
}

pub(crate) fn open_file_content_for_entry(
    conn: &Connection,
    repo: &FileRepo<'_>,
    entry: &FileEntry,
) -> Result<Box<dyn Read>, FileServiceError> {
    validate_readable_file_entry(conn, entry)?;
    let (kind, source_path) = source_location(repo, entry)?;
    match kind.as_str() {
        "logical_directory" => open_logical_file(&source_path, entry),
        "e01" => {
            let partition_index = resolve_partition_index_for_entry(repo, entry)?;
            open_e01_file(conn, &source_path, entry, partition_index)
        }
        "raw" => {
            let partition_index = resolve_partition_index_for_entry(repo, entry)?;
            open_raw_file(&source_path, entry, partition_index)
        }
        other => unsupported_source(other),
    }
}

pub(crate) fn open_range_content_for_entry(
    conn: &Connection,
    repo: &FileRepo<'_>,
    entry: &FileEntry,
) -> Result<RangeContentReader, FileServiceError> {
    validate_readable_file_entry(conn, entry)?;
    let (kind, source_path) = source_location(repo, entry)?;
    match kind.as_str() {
        "logical_directory" => {
            open_logical_file_seekable(&source_path, entry).map(RangeContentReader::Seekable)
        }
        "e01" => {
            let partition_index = resolve_partition_index_for_entry(repo, entry)?;
            open_e01_file(conn, &source_path, entry, partition_index)
                .map(RangeContentReader::Streaming)
        }
        "raw" => {
            let partition_index = resolve_partition_index_for_entry(repo, entry)?;
            open_raw_file(&source_path, entry, partition_index).map(RangeContentReader::Streaming)
        }
        other => unsupported_source(other),
    }
}

fn source_location(
    repo: &FileRepo<'_>,
    entry: &FileEntry,
) -> Result<(String, String), FileServiceError> {
    repo.find_data_source_location(&entry.data_source_id)?
        .ok_or_else(|| FileServiceError::not_found("Data source not found"))
}

fn open_logical_file(
    source_path: &str,
    entry: &FileEntry,
) -> Result<Box<dyn Read>, FileServiceError> {
    Ok(Box::new(std::fs::File::open(resolve_logical_file_path(
        source_path,
        entry,
    )?)?))
}

fn open_logical_file_seekable(
    source_path: &str,
    entry: &FileEntry,
) -> Result<Box<dyn evidence_core::ReadSeek>, FileServiceError> {
    Ok(Box::new(std::fs::File::open(resolve_logical_file_path(
        source_path,
        entry,
    )?)?))
}

fn resolve_logical_file_path(
    source_path: &str,
    entry: &FileEntry,
) -> Result<std::path::PathBuf, FileServiceError> {
    let root = std::path::PathBuf::from(source_path).canonicalize()?;
    let relative_path = crate::file_service::viewer::safe_relative_path(&entry.path)?;
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
            return Err(FileServiceError::other(format!(
                "Symlink detected in path at '{}' - rejected for security",
                current.display()
            )));
        }
    }
    Ok(())
}

fn unsupported_source<T>(kind: &str) -> Result<T, FileServiceError> {
    Err(FileServiceError::other(format!(
        "Range reading is not yet wired for data source kind '{kind}'"
    )))
}
