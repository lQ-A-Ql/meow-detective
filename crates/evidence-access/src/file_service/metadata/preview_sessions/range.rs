use std::{path::Path, sync::Arc};

use base64::Engine;
use domain::CaseId;
use transport::dto::{
    MediaRangeRequestDto, MediaRangeResponseDto, ViewerRangeRequestDto, ViewerRangeResponseDto,
};

use crate::file_service::{
    metadata::source_routing::read_file_range_for_source_case, FileServiceError,
    PreviewRuntimeRegistry,
};

pub fn read_preview_session_range_for_case(
    registry: &PreviewRuntimeRegistry,
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    case_id: &CaseId,
    request: &ViewerRangeRequestDto,
) -> Result<ViewerRangeResponseDto, FileServiceError> {
    read_preview_session_range_internal(registry, case_conn, case_root, case_id, request, None)
}

pub fn read_preview_session_range_for_case_with_bitlocker(
    bitlocker_runtime: &Arc<crate::bitlocker_runtime::BitLockerUnlockRegistry>,
    registry: &PreviewRuntimeRegistry,
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    case_id: &CaseId,
    request: &ViewerRangeRequestDto,
) -> Result<ViewerRangeResponseDto, FileServiceError> {
    read_preview_session_range_internal(
        registry,
        case_conn,
        case_root,
        case_id,
        request,
        Some(bitlocker_runtime),
    )
}

fn read_preview_session_range_internal(
    registry: &PreviewRuntimeRegistry,
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    case_id: &CaseId,
    request: &ViewerRangeRequestDto,
    bitlocker_runtime: Option<&Arc<crate::bitlocker_runtime::BitLockerUnlockRegistry>>,
) -> Result<ViewerRangeResponseDto, FileServiceError> {
    let mut request = request.clone();
    request.validate().map_err(FileServiceError::InvalidInput)?;
    let session = registry.get_session(&case_id.0, &request.handle_id)?;
    if request.offset >= session.size() {
        return Ok(empty_viewer_range(request.offset));
    }
    let length = (request.length as usize).min(infrastructure::constants::MAX_RANGE_LENGTH);
    match session.read_prepared_range(request.offset, length) {
        Ok(Some(bytes)) => return Ok(viewer_range_bytes(&session, request.offset, bytes)),
        Ok(None) => {}
        Err(error) if prepared_reader_can_fallback(&error) => {
            tracing::warn!(error = %error, "Prepared preview reader failed; retrying through the source-routed reader");
        }
        Err(error) => return Err(error),
    }
    request.handle_id = format!("file:{}", session.global_file_id());
    let routed = match bitlocker_runtime {
        Some(runtime) => crate::file_service::read_file_range_for_source_case_with_bitlocker(
            runtime, case_conn, case_root, case_id, &request,
        ),
        None => read_file_range_for_source_case(case_conn, case_root, case_id, &request),
    };
    routed.map_err(map_routed_preview_error)
}

fn empty_viewer_range(offset: u64) -> ViewerRangeResponseDto {
    ViewerRangeResponseDto {
        offset,
        bytes_read: 0,
        eof: true,
        raw_bytes: Some(Vec::new()),
        kind: "hex".to_string(),
        lines: Vec::new(),
        encoding: None,
    }
}

fn viewer_range_bytes(
    session: &crate::file_service::preview_runtime::PreviewSession,
    offset: u64,
    bytes: Vec<u8>,
) -> ViewerRangeResponseDto {
    ViewerRangeResponseDto {
        offset,
        bytes_read: bytes.len() as u32,
        eof: offset.saturating_add(bytes.len() as u64) >= session.size(),
        raw_bytes: Some(bytes),
        kind: "hex".to_string(),
        lines: Vec::new(),
        encoding: None,
    }
}

fn map_routed_preview_error(error: FileServiceError) -> FileServiceError {
    match error {
        FileServiceError::InvalidInput(_)
        | FileServiceError::Db(_)
        | FileServiceError::Io(_)
        | FileServiceError::NotFound(_)
        | FileServiceError::Security(_)
        | FileServiceError::PathTraversal(_)
        | FileServiceError::Integrity(_) => error,
        _ => FileServiceError::other("Preview routed read failed"),
    }
}

fn prepared_reader_can_fallback(error: &FileServiceError) -> bool {
    matches!(error, FileServiceError::Io(_) | FileServiceError::Other(_))
}

pub fn read_preview_session_media_range_for_case(
    registry: &PreviewRuntimeRegistry,
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    case_id: &CaseId,
    request: &MediaRangeRequestDto,
) -> Result<MediaRangeResponseDto, FileServiceError> {
    read_preview_session_media_range_internal(
        registry, case_conn, case_root, case_id, request, None,
    )
}

pub fn read_preview_session_media_range_for_case_with_bitlocker(
    bitlocker_runtime: &Arc<crate::bitlocker_runtime::BitLockerUnlockRegistry>,
    registry: &PreviewRuntimeRegistry,
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    case_id: &CaseId,
    request: &MediaRangeRequestDto,
) -> Result<MediaRangeResponseDto, FileServiceError> {
    read_preview_session_media_range_internal(
        registry,
        case_conn,
        case_root,
        case_id,
        request,
        Some(bitlocker_runtime),
    )
}

fn read_preview_session_media_range_internal(
    registry: &PreviewRuntimeRegistry,
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    case_id: &CaseId,
    request: &MediaRangeRequestDto,
    bitlocker_runtime: Option<&Arc<crate::bitlocker_runtime::BitLockerUnlockRegistry>>,
) -> Result<MediaRangeResponseDto, FileServiceError> {
    let session = registry.get_session(&case_id.0, &request.handle_id)?;
    if request.offset >= session.size() {
        return Ok(MediaRangeResponseDto {
            offset: request.offset,
            bytes_base64: String::new(),
            bytes_read: 0,
            eof: true,
        });
    }
    let readable_len = request
        .length
        .min(transport::dto::MAX_VIEWER_RANGE_LENGTH)
        .min((session.size() - request.offset).min(u32::MAX as u64) as u32);
    let range_request = ViewerRangeRequestDto {
        handle_id: request.handle_id.clone(),
        offset: request.offset,
        length: readable_len,
    };
    let response = match bitlocker_runtime {
        Some(runtime) => read_preview_session_range_for_case_with_bitlocker(
            runtime,
            registry,
            case_conn,
            case_root,
            case_id,
            &range_request,
        )?,
        None => read_preview_session_range_for_case(
            registry,
            case_conn,
            case_root,
            case_id,
            &range_request,
        )?,
    };
    let bytes = response
        .raw_bytes
        .ok_or_else(|| FileServiceError::other("Preview session returned no bytes"))?;
    let bytes_read = bytes.len();
    Ok(MediaRangeResponseDto {
        offset: request.offset,
        bytes_base64: base64::engine::general_purpose::STANDARD.encode(bytes),
        bytes_read: bytes_read as u32,
        eof: request.offset.saturating_add(bytes_read as u64) >= session.size(),
    })
}

pub fn read_preview_session_bytes_for_case(
    registry: &PreviewRuntimeRegistry,
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    case_id: &CaseId,
    handle_id: &str,
    offset: u64,
    length: u32,
) -> Result<Vec<u8>, FileServiceError> {
    let request = ViewerRangeRequestDto {
        handle_id: handle_id.to_string(),
        offset,
        length,
    };
    let response =
        read_preview_session_range_for_case(registry, case_conn, case_root, case_id, &request)?;
    response
        .raw_bytes
        .ok_or_else(|| FileServiceError::other("Preview session returned no bytes"))
}
