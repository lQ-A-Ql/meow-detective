use std::io::Read;

use domain::{FileEntry, FileEntryId};
use persistence_sqlite::repositories::file_repo::FileRepo;
use rusqlite::Connection;
use transport::dto::{ViewerRangeRequestDto, ViewerRangeResponseDto};

use crate::file_service::{
    viewer::{
        descriptor_for_file_with_cache, format_image_range_error, read_bounded,
        read_seekable_range, try_read_exfat_image_range_for_descriptor,
        try_read_exfat_image_range_for_entry, try_read_fat_image_range_for_descriptor,
        try_read_fat_image_range_for_entry, try_read_linux_image_range_for_descriptor,
        try_read_linux_image_range_for_entry, try_read_ntfs_image_range_for_descriptor,
        try_read_ntfs_image_range_for_entry, PreviewDescriptor, PreviewReadContext,
        RangeContentReader, FILE_HANDLE_PREFIX,
    },
    FileServiceError,
};

use super::content::{
    open_file_content_for_descriptor_with_context, open_file_content_for_entry,
    open_range_content_for_descriptor, open_range_content_for_descriptor_with_context,
    open_range_content_for_entry,
};

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
    if context.case_id().is_empty() {
        let repo = FileRepo::new(context.conn());
        let entry = repo
            .find_by_id(file_id)?
            .ok_or_else(|| FileServiceError::not_found("File not found"))?;
        return open_file_content_for_entry(context.conn(), &repo, &entry);
    }
    let descriptor = descriptor_for_file_with_cache(&mut context, file_id)?;
    open_file_content_for_descriptor_with_context(&mut context, &descriptor)
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
    if context.case_id().is_empty() {
        let repo = FileRepo::new(context.conn());
        let entry = repo
            .find_by_id(file_id)?
            .ok_or_else(|| FileServiceError::not_found("File not found"))?;
        if entry.size.is_some_and(|size| offset > size) {
            return Err(FileServiceError::other("Read offset exceeds file size"));
        }
        return read_file_bytes_for_entry(context.conn(), &repo, &entry, offset, length);
    }
    let descriptor = descriptor_for_file_with_cache(&mut context, file_id)?;
    if offset > descriptor.size {
        return Err(FileServiceError::other("Read offset exceeds file size"));
    }
    read_file_bytes_for_descriptor_with_context(&mut context, &descriptor, offset, length)
}

fn read_file_bytes_for_entry(
    conn: &Connection,
    repo: &FileRepo<'_>,
    entry: &FileEntry,
    offset: u64,
    length: u32,
) -> Result<Vec<u8>, FileServiceError> {
    let length = clamp_range_length(length);
    if let Some(bytes) = try_read_ntfs_image_range_for_entry(conn, repo, entry, offset, length)? {
        return Ok(bytes);
    }
    if let Some(bytes) = try_read_fat_image_range_for_entry(conn, repo, entry, offset, length)? {
        return Ok(bytes);
    }
    if let Some(bytes) = try_read_exfat_image_range_for_entry(conn, repo, entry, offset, length)? {
        return Ok(bytes);
    }
    if let Some(bytes) = try_read_linux_image_range_for_entry(conn, repo, entry, offset, length)? {
        return Ok(bytes);
    }
    read_from_range_reader(
        open_range_content_for_entry(conn, repo, entry)?,
        offset,
        length,
    )
}

pub(crate) fn read_file_bytes_for_descriptor_with_context<C>(
    context: &mut C,
    descriptor: &PreviewDescriptor,
    offset: u64,
    length: u32,
) -> Result<Vec<u8>, FileServiceError>
where
    C: PreviewReadContext,
{
    let length = clamp_range_length(length);
    if descriptor.source_kind == "ceph_rbd" {
        return match open_range_content_for_descriptor_with_context(context, descriptor)? {
            RangeContentReader::Seekable(mut reader) => {
                read_seekable_range(reader.as_mut(), offset, length)
            }
            RangeContentReader::Streaming(mut reader) => {
                crate::file_service::viewer::skip_reader_bytes(reader.as_mut(), offset)?;
                read_bounded(reader.as_mut(), length)
            }
        };
    }
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
        Ok(reader) => read_from_range_reader(reader, offset, length),
        Err(error) if reasons.is_empty() => Err(error),
        Err(error) => Err(FileServiceError::other(format_image_range_error(
            &descriptor.path,
            &reasons,
            Some(&error.to_string()),
        ))),
    }
}

fn read_from_range_reader(
    reader: RangeContentReader,
    offset: u64,
    length: usize,
) -> Result<Vec<u8>, FileServiceError> {
    match reader {
        RangeContentReader::Seekable(mut reader) => {
            read_seekable_range(reader.as_mut(), offset, length)
        }
        RangeContentReader::Streaming(mut reader) => {
            crate::file_service::viewer::skip_reader_bytes(reader.as_mut(), offset)?;
            read_bounded(reader.as_mut(), length)
        }
    }
}

fn clamp_range_length(length: u32) -> usize {
    (length as usize).min(infrastructure::constants::MAX_RANGE_LENGTH)
}

pub(crate) fn file_id_from_handle(handle_id: &str) -> Result<&str, FileServiceError> {
    handle_id
        .strip_prefix(FILE_HANDLE_PREFIX)
        .filter(|file_id| !file_id.is_empty())
        .ok_or_else(|| FileServiceError::invalid_input("Invalid file handle"))
}
