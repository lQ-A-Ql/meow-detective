//! Stream evidence content to a verified host filesystem destination.

use std::path::Path;

use domain::FileEntryId;
use persistence_sqlite::repositories::file_repo::FileRepo;
use rusqlite::Connection;
use transport::dto::FileExtractionResultDto;

use crate::file_service::{source_read::SourceExtractionMode, FileServiceError, SourceReadContext};

use super::{copy, policy};

pub fn extract_file_to_destination(
    conn: &Connection,
    file_id: &str,
    destination_path: &Path,
    overwrite: bool,
) -> Result<FileExtractionResultDto, FileServiceError> {
    let entry = FileRepo::new(conn)
        .find_by_id(&FileEntryId(file_id.to_string()))?
        .ok_or_else(|| FileServiceError::not_found("File not found"))?;
    let mut reader = crate::file_service::open_file_content_by_id(conn, &entry.id)?;
    let destination = policy::prepare_destination(
        destination_path,
        overwrite,
        policy::DestinationScope::Unscoped,
    )?;
    let copied = copy::copy_reader_to_destination(
        reader.as_mut(),
        entry.size,
        &destination,
        overwrite,
        None,
    )?;
    Ok(extraction_result(file_id, entry.size, &destination, copied))
}

pub(crate) struct SourceExtractionRequest<'a, 'progress> {
    pub(crate) global_file_id: &'a str,
    pub(crate) local_file_id: &'a FileEntryId,
    pub(crate) source_size: Option<u64>,
    pub(crate) destination_path: &'a Path,
    pub(crate) overwrite: bool,
    pub(crate) destination_scope: policy::DestinationScope<'a>,
    pub(crate) progress: Option<copy::CopyProgressCallback<'progress>>,
}

pub(crate) fn extract_source_file(
    context: &mut SourceReadContext<'_>,
    request: SourceExtractionRequest<'_, '_>,
) -> Result<FileExtractionResultDto, FileServiceError> {
    validate_source_entry(context, request.local_file_id)?;
    let destination = policy::prepare_destination(
        request.destination_path,
        request.overwrite,
        request.destination_scope,
    )?;
    let plan = context.extraction_plan_by_id(request.local_file_id)?;
    if request.source_size.is_some_and(|size| size != plan.size) {
        return Err(FileServiceError::integrity(
            "File catalog size changed while preparing extraction",
        ));
    }
    let source_size = request.source_size.or(Some(plan.size));
    let copied = match plan.mode {
        SourceExtractionMode::Reader(mut reader) => {
            let reader: &mut dyn std::io::Read = match &mut reader {
                crate::file_service::RangeContentReader::Seekable(reader) => reader.as_mut(),
                crate::file_service::RangeContentReader::Streaming(reader) => reader.as_mut(),
            };
            copy::copy_reader_to_destination(
                reader,
                source_size,
                &destination,
                request.overwrite,
                request.progress,
            )?
        }
        SourceExtractionMode::Chunked => {
            let size = source_size.ok_or_else(|| {
                FileServiceError::integrity("Chunked evidence source has no catalog size")
            })?;
            copy::copy_chunks_to_destination(
                size,
                &destination,
                request.overwrite,
                request.progress,
                |offset, length| {
                    context.read_extraction_chunk_by_id(request.local_file_id, offset, length)
                },
            )?
        }
    };
    Ok(extraction_result(
        request.global_file_id,
        source_size,
        &destination,
        copied,
    ))
}

fn validate_source_entry(
    context: &SourceReadContext<'_>,
    file_id: &FileEntryId,
) -> Result<(), FileServiceError> {
    let repo = FileRepo::new(context.source_connection());
    let entry = repo
        .find_by_id(file_id)?
        .ok_or_else(|| FileServiceError::not_found("File not found"))?;
    crate::file_service::viewer::validate_readable_file_entry(context.source_connection(), &entry)?;
    Ok(())
}

fn extraction_result(
    file_id: &str,
    source_size: Option<u64>,
    destination: &Path,
    copied: copy::StreamCopyResult,
) -> FileExtractionResultDto {
    let destination_file_name = destination
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "extracted-file".to_string());
    FileExtractionResultDto {
        file_id: file_id.to_string(),
        bytes_written: copied.bytes_written,
        source_size,
        sha256: copied.sha256,
        destination_file_name,
        size_verified: source_size.is_some_and(|size| size == copied.bytes_written),
    }
}
