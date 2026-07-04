//! Public range-read and content-open API used by Tauri commands.

use crate::file_service::viewer::{
    descriptor_file_entry, descriptor_for_file_with_cache, read_bounded, read_seekable_range,
    try_read_exfat_image_range_for_descriptor, try_read_fat_image_range_for_descriptor,
    try_read_linux_image_range_for_descriptor, try_read_ntfs_image_range_for_descriptor,
    PreviewDescriptor, PreviewReadContext, RangeContentReader, FILE_HANDLE_PREFIX,
};
use crate::file_service::FileServiceError;
use domain::{EntryType, FileEntry, FileEntryId};
use persistence_sqlite::repositories::file_repo::FileRepo;
use rusqlite::Connection;
use serde_json::Value;
use std::cell::RefCell;
use std::collections::HashMap;
use std::io::Read;
use transport::dto::{ViewerRangeRequestDto, ViewerRangeResponseDto};

pub fn read_file_range_for_case<C>(
    context: C,
    request: &ViewerRangeRequestDto,
) -> Result<ViewerRangeResponseDto, FileServiceError>
where
    C: PreviewReadContext,
{
    let mut request = request.clone();
    request.validate().map_err(FileServiceError::InvalidInput)?;
    let file_id = file_id_from_handle(&request.handle_id)?;
    let bytes = read_file_bytes_for_case(
        context,
        &FileEntryId(file_id.to_string()),
        request.offset,
        request.length,
    )?;

    Ok(ViewerRangeResponseDto {
        raw_bytes: Some(bytes),
        kind: "hex".into(),
        lines: Vec::new(),
        encoding: None,
    })
}

pub fn open_file_content_by_id<C>(
    mut context: C,
    file_id: &FileEntryId,
) -> Result<Box<dyn Read>, FileServiceError>
where
    C: PreviewReadContext,
{
    #[cfg(test)]
    crate::file_service::viewer::OPEN_FILE_CONTENT_BY_ID_CALLS
        .with(|calls| calls.set(calls.get() + 1));

    if context.case_id().is_empty() {
        let repo = FileRepo::new(context.conn());
        let entry = repo
            .find_by_id(file_id)?
            .ok_or_else(|| FileServiceError::not_found("File not found"))?;

        return open_file_content_for_entry(context.conn(), &repo, &entry);
    }

    let descriptor = descriptor_for_file_with_cache(&mut context, file_id)?;
    open_file_content_for_descriptor(&descriptor)
}

pub fn read_file_bytes_for_case<C>(
    mut context: C,
    file_id: &FileEntryId,
    offset: u64,
    length: u32,
) -> Result<Vec<u8>, FileServiceError>
where
    C: PreviewReadContext,
{
    #[cfg(test)]
    crate::file_service::viewer::READ_FILE_BYTES_FOR_CASE_CALLS
        .with(|calls| calls.set(calls.get() + 1));

    if context.case_id().is_empty() {
        let repo = FileRepo::new(context.conn());
        let entry = repo
            .find_by_id(file_id)?
            .ok_or_else(|| FileServiceError::not_found("File not found"))?;
        if let Some(size) = entry.size {
            if offset > size {
                return Err(FileServiceError::other("Read offset exceeds file size"));
            }
        }

        return read_file_bytes_for_entry(context.conn(), &repo, &entry, offset, length);
    }

    let descriptor = descriptor_for_file_with_cache(&mut context, file_id)?;
    if offset > descriptor.size {
        return Err(FileServiceError::other("Read offset exceeds file size"));
    }

    read_file_bytes_for_descriptor(&descriptor, offset, length)
}

pub(crate) fn read_file_bytes_for_entry(
    conn: &Connection,
    repo: &FileRepo<'_>,
    entry: &FileEntry,
    offset: u64,
    length: u32,
) -> Result<Vec<u8>, FileServiceError> {
    let length = (length as usize).min(infrastructure::constants::MAX_RANGE_LENGTH);
    if let Some(bytes) = crate::file_service::viewer::try_read_ntfs_image_range_for_entry(
        conn, repo, entry, offset, length,
    )? {
        return Ok(bytes);
    }
    if let Some(bytes) = crate::file_service::viewer::try_read_fat_image_range_for_entry(
        conn, repo, entry, offset, length,
    )? {
        return Ok(bytes);
    }
    if let Some(bytes) = crate::file_service::viewer::try_read_exfat_image_range_for_entry(
        conn, repo, entry, offset, length,
    )? {
        return Ok(bytes);
    }
    if let Some(bytes) = crate::file_service::viewer::try_read_linux_image_range_for_entry(
        conn, repo, entry, offset, length,
    )? {
        return Ok(bytes);
    }

    match open_range_content_for_entry(conn, repo, entry)? {
        RangeContentReader::Seekable(mut reader) => {
            read_seekable_range(reader.as_mut(), offset, length)
        }
        RangeContentReader::Streaming(mut reader) => {
            // Image-backed filesystem readers still expose `Read` only and may
            // materialize file data internally. Keep this compatibility path
            // until fs-* crates expose seekable per-file streams.
            crate::file_service::viewer::skip_reader_bytes(reader.as_mut(), offset)?;
            read_bounded(reader.as_mut(), length)
        }
    }
}

pub fn read_file_bytes_for_descriptor(
    descriptor: &PreviewDescriptor,
    offset: u64,
    length: u32,
) -> Result<Vec<u8>, FileServiceError> {
    let length = (length as usize).min(infrastructure::constants::MAX_RANGE_LENGTH);
    let mut reasons = Vec::new();
    if let Some(bytes) =
        try_read_ntfs_image_range_for_descriptor(descriptor, offset, length, &mut reasons)?
    {
        return Ok(bytes);
    }
    if let Some(bytes) =
        try_read_fat_image_range_for_descriptor(descriptor, offset, length, &mut reasons)?
    {
        return Ok(bytes);
    }
    if let Some(bytes) =
        try_read_exfat_image_range_for_descriptor(descriptor, offset, length, &mut reasons)?
    {
        return Ok(bytes);
    }
    if let Some(bytes) =
        try_read_linux_image_range_for_descriptor(descriptor, offset, length, &mut reasons)?
    {
        return Ok(bytes);
    }

    match open_range_content_for_descriptor(descriptor) {
        Ok(RangeContentReader::Seekable(mut reader)) => {
            read_seekable_range(reader.as_mut(), offset, length)
        }
        Ok(RangeContentReader::Streaming(mut reader)) => {
            crate::file_service::viewer::skip_reader_bytes(reader.as_mut(), offset)?;
            read_bounded(reader.as_mut(), length)
        }
        Err(error) => {
            if reasons.is_empty() {
                return Err(error);
            }
            Err(FileServiceError::other(
                crate::file_service::viewer::format_image_range_error(
                    &descriptor.path,
                    &reasons,
                    Some(&error.to_string()),
                ),
            ))
        }
    }
}

pub(crate) fn open_file_content_for_descriptor(
    descriptor: &PreviewDescriptor,
) -> Result<Box<dyn Read>, FileServiceError> {
    let range_reader = match descriptor.source_kind.as_str() {
        "logical_directory" => open_logical_descriptor_file(descriptor),
        "e01" => open_e01_descriptor_file(descriptor),
        "raw" => open_raw_descriptor_file(descriptor),
        other => Err(FileServiceError::other(format!(
            "Range reading is not yet wired for data source kind '{}'",
            other
        ))),
    }?;
    Ok(match range_reader {
        RangeContentReader::Seekable(reader) => reader as Box<dyn Read>,
        RangeContentReader::Streaming(reader) => reader,
    })
}

pub(crate) fn open_logical_descriptor_file(
    descriptor: &PreviewDescriptor,
) -> Result<RangeContentReader, FileServiceError> {
    let entry = descriptor_file_entry(descriptor);
    open_logical_file_seekable(&descriptor.source_path, &entry).map(RangeContentReader::Seekable)
}

pub(crate) fn open_logical_descriptor_seekable(
    descriptor: &PreviewDescriptor,
) -> Result<Box<dyn evidence_core::ReadSeek>, FileServiceError> {
    let entry = descriptor_file_entry(descriptor);
    open_logical_file_seekable(&descriptor.source_path, &entry)
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
        other => Err(FileServiceError::other(format!(
            "Range reading is not yet wired for data source kind '{}'",
            other
        ))),
    }
}

pub(crate) fn open_e01_descriptor_file(
    descriptor: &PreviewDescriptor,
) -> Result<RangeContentReader, FileServiceError> {
    let case_id = descriptor.case_id.clone();
    crate::file_service::viewer::open_descriptor_image_file(descriptor, move |source_path| {
        crate::file_service::viewer::open_e01_reader_cached(source_path, &case_id)
            .map(|reader| Box::new(reader) as Box<dyn evidence_core::EvidenceReader>)
    })
}

pub(crate) fn open_raw_descriptor_file(
    descriptor: &PreviewDescriptor,
) -> Result<RangeContentReader, FileServiceError> {
    crate::file_service::viewer::open_descriptor_image_file(descriptor, |source_path| {
        evidence_core::RawImageReader::open(source_path)
            .map(|reader| Box::new(reader) as Box<dyn evidence_core::EvidenceReader>)
    })
}

pub(crate) fn open_file_content_for_entry(
    conn: &Connection,
    repo: &FileRepo<'_>,
    entry: &FileEntry,
) -> Result<Box<dyn Read>, FileServiceError> {
    if entry.entry_type != EntryType::File {
        return Err(FileServiceError::invalid_input(
            "Cannot read a directory as a file",
        ));
    }

    let (kind, source_path) = repo
        .find_data_source_location(&entry.data_source_id)?
        .ok_or_else(|| FileServiceError::not_found("Data source not found"))?;
    let expected_partition_index =
        crate::file_service::viewer::root_partition_index_for_entry(repo, entry);

    match kind.as_str() {
        "logical_directory" => open_logical_file(&source_path, entry),
        "e01" => crate::file_service::viewer::open_e01_file(
            conn,
            &source_path,
            entry,
            expected_partition_index,
        ),
        "raw" => crate::file_service::viewer::open_raw_file(
            &source_path,
            entry,
            expected_partition_index,
        ),
        other => Err(FileServiceError::other(format!(
            "Range reading is not yet wired for data source kind '{}'",
            other
        ))),
    }
}

pub(crate) fn open_range_content_for_entry(
    conn: &Connection,
    repo: &FileRepo<'_>,
    entry: &FileEntry,
) -> Result<RangeContentReader, FileServiceError> {
    if entry.entry_type != EntryType::File {
        return Err(FileServiceError::invalid_input(
            "Cannot read a directory as a file",
        ));
    }

    let (kind, source_path) = repo
        .find_data_source_location(&entry.data_source_id)?
        .ok_or_else(|| FileServiceError::not_found("Data source not found"))?;
    let expected_partition_index =
        crate::file_service::viewer::root_partition_index_for_entry(repo, entry);

    match kind.as_str() {
        "logical_directory" => {
            open_logical_file_seekable(&source_path, entry).map(RangeContentReader::Seekable)
        }
        "e01" => crate::file_service::viewer::open_e01_file(
            conn,
            &source_path,
            entry,
            expected_partition_index,
        )
        .map(RangeContentReader::Streaming),
        "raw" => crate::file_service::viewer::open_raw_file(
            &source_path,
            entry,
            expected_partition_index,
        )
        .map(RangeContentReader::Streaming),
        other => Err(FileServiceError::other(format!(
            "Range reading is not yet wired for data source kind '{}'",
            other
        ))),
    }
}

fn open_logical_file(
    source_path: &str,
    entry: &FileEntry,
) -> Result<Box<dyn Read>, FileServiceError> {
    Ok(Box::new(std::fs::File::open(resolve_logical_file_path(
        source_path,
        entry,
    )?)?) as Box<dyn Read>)
}

fn open_logical_file_seekable(
    source_path: &str,
    entry: &FileEntry,
) -> Result<Box<dyn evidence_core::ReadSeek>, FileServiceError> {
    Ok(Box::new(std::fs::File::open(resolve_logical_file_path(
        source_path,
        entry,
    )?)?) as Box<dyn evidence_core::ReadSeek>)
}

fn resolve_logical_file_path(
    source_path: &str,
    entry: &FileEntry,
) -> Result<std::path::PathBuf, FileServiceError> {
    let root = std::path::PathBuf::from(source_path).canonicalize()?;
    let relative_path = crate::file_service::viewer::safe_relative_path(&entry.path)?;
    let full_path = root.join(&relative_path);

    let mut check_path = std::path::PathBuf::new();
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

pub fn read_file_header_by_id(
    conn: &Connection,
    file_id: &FileEntryId,
    max_bytes: usize,
) -> Result<Vec<u8>, FileServiceError> {
    let mut bytes = Vec::with_capacity(max_bytes.min(infrastructure::constants::MAX_RANGE_LENGTH));
    let mut offset = 0u64;
    let mut remaining = max_bytes;

    while remaining > 0 {
        let chunk_len = remaining
            .min(infrastructure::constants::MAX_RANGE_LENGTH)
            .min(u32::MAX as usize) as u32;
        if chunk_len == 0 {
            break;
        }

        let chunk = match read_file_bytes_for_case(conn, file_id, offset, chunk_len) {
            Ok(chunk) => chunk,
            Err(error) if error.is_read_offset_beyond_size() => break,
            Err(error) => return Err(error),
        };
        if chunk.is_empty() {
            break;
        }

        let is_short_read = chunk.len() < chunk_len as usize;
        offset = offset.saturating_add(chunk.len() as u64);
        remaining = remaining.saturating_sub(chunk.len());
        bytes.extend_from_slice(&chunk);

        if is_short_read {
            break;
        }
    }

    Ok(bytes)
}

pub struct FileHeaderReadCache {
    case_id: String,
    descriptors: RefCell<HashMap<String, Value>>,
}

impl FileHeaderReadCache {
    pub fn new(case_id: impl Into<String>) -> Self {
        Self {
            case_id: case_id.into(),
            descriptors: RefCell::new(HashMap::new()),
        }
    }

    pub fn read_file_header_by_id(
        &self,
        conn: &Connection,
        file_id: &FileEntryId,
        max_bytes: usize,
    ) -> Result<Vec<u8>, FileServiceError> {
        if self.case_id.is_empty() {
            return read_file_header_by_id(conn, file_id, max_bytes);
        }

        let mut bytes =
            Vec::with_capacity(max_bytes.min(infrastructure::constants::MAX_RANGE_LENGTH));
        let mut offset = 0u64;
        let mut remaining = max_bytes;

        while remaining > 0 {
            let chunk_len = remaining
                .min(infrastructure::constants::MAX_RANGE_LENGTH)
                .min(u32::MAX as usize) as u32;
            if chunk_len == 0 {
                break;
            }

            let get_cache = |key: &str| self.descriptors.borrow().get(key).cloned();
            let set_cache = |key: &str, value: &Value| {
                self.descriptors
                    .borrow_mut()
                    .insert(key.to_string(), value.clone());
            };
            let chunk = match read_file_bytes_for_case(
                (conn, self.case_id.as_str(), get_cache, set_cache),
                file_id,
                offset,
                chunk_len,
            ) {
                Ok(chunk) => chunk,
                Err(error) if error.is_read_offset_beyond_size() => break,
                Err(error) => return Err(error),
            };
            if chunk.is_empty() {
                break;
            }

            let is_short_read = chunk.len() < chunk_len as usize;
            offset = offset.saturating_add(chunk.len() as u64);
            remaining = remaining.saturating_sub(chunk.len());
            bytes.extend_from_slice(&chunk);
            if is_short_read {
                break;
            }
        }

        Ok(bytes)
    }
}

pub(crate) fn file_id_from_handle(handle_id: &str) -> Result<&str, FileServiceError> {
    handle_id
        .strip_prefix(FILE_HANDLE_PREFIX)
        .filter(|file_id| !file_id.is_empty())
        .ok_or_else(|| FileServiceError::invalid_input("Invalid file handle"))
}
