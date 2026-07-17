use std::path::Path;

use base64::Engine;
use domain::{CaseId, DataSourceId};
use transport::dto::{
    MediaRangeRequestDto, MediaRangeResponseDto, ViewerHandleDto, ViewerRangeRequestDto,
    ViewerRangeResponseDto,
};

use crate::file_service::{
    metadata::source_routing::{open_source_for_file_id, read_file_range_for_source_case},
    preview_runtime::PreviewSession,
    viewer::preview_descriptor_for_case,
    FileServiceError, PreviewRuntimeRegistry,
};

pub fn open_preview_session_for_case(
    registry: &PreviewRuntimeRegistry,
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    case_id: &CaseId,
    file_id: &str,
) -> Result<ViewerHandleDto, FileServiceError> {
    let (global_id, source_conn) = open_source_for_file_id(case_conn, case_root, case_id, file_id)?;
    let scope = registry.begin_session(case_id, &global_id.data_source_id)?;
    let descriptor = preview_descriptor_for_case(&source_conn, &case_id.0, &global_id.local_id)?;
    let global_file_id =
        crate::source_db::GlobalFileId::new(global_id.data_source_id.clone(), global_id.local_id)
            .encode()
            .0;

    let session = if descriptor.source_kind == "ceph_rbd" {
        let runtime = registry.resolve_derived_runtime(
            case_conn,
            case_root,
            case_id,
            &global_id.data_source_id,
            &scope,
        )?;
        PreviewSession::prepared_ceph(
            case_id.0.clone(),
            global_file_id,
            descriptor.size,
            descriptor.mime.clone(),
            runtime,
            &descriptor,
        )?
    } else {
        PreviewSession::routed(
            case_id.0.clone(),
            global_id.data_source_id.0,
            global_file_id,
            descriptor.size,
            descriptor.mime.clone(),
        )
    };
    let handle_id = registry.insert_session(&scope, session)?;
    Ok(ViewerHandleDto {
        handle_id,
        size: descriptor.size,
        mime: descriptor.mime,
    })
}

pub fn read_preview_session_range_for_case(
    registry: &PreviewRuntimeRegistry,
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    case_id: &CaseId,
    request: &ViewerRangeRequestDto,
) -> Result<ViewerRangeResponseDto, FileServiceError> {
    let mut request = request.clone();
    request.validate().map_err(FileServiceError::InvalidInput)?;
    let session = registry.get_session(&case_id.0, &request.handle_id)?;
    let length = (request.length as usize).min(infrastructure::constants::MAX_RANGE_LENGTH);
    if let Some(bytes) = session.read_prepared_range(request.offset, length)? {
        return Ok(ViewerRangeResponseDto {
            raw_bytes: Some(bytes),
            kind: "hex".to_string(),
            lines: Vec::new(),
            encoding: None,
        });
    }

    request.handle_id = format!("file:{}", session.global_file_id());
    read_file_range_for_source_case(case_conn, case_root, case_id, &request)
}

pub fn read_preview_session_media_range_for_case(
    registry: &PreviewRuntimeRegistry,
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    case_id: &CaseId,
    request: &MediaRangeRequestDto,
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
    let bytes = read_preview_session_bytes_for_case(
        registry,
        case_conn,
        case_root,
        case_id,
        &request.handle_id,
        request.offset,
        readable_len,
    )?;
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
    let response = read_preview_session_range_for_case(
        registry,
        case_conn,
        case_root,
        case_id,
        &ViewerRangeRequestDto {
            handle_id: handle_id.to_string(),
            offset,
            length,
        },
    )?;
    response
        .raw_bytes
        .ok_or_else(|| FileServiceError::other("Preview session returned no bytes"))
}

pub fn close_preview_session_for_case(
    registry: &PreviewRuntimeRegistry,
    case_id: &CaseId,
    handle_id: &str,
) -> Result<bool, FileServiceError> {
    registry.close_session(&case_id.0, handle_id)
}

pub fn preview_session_file_id(
    registry: &PreviewRuntimeRegistry,
    case_id: &CaseId,
    handle_id: &str,
) -> Result<String, FileServiceError> {
    registry
        .get_session(&case_id.0, handle_id)
        .map(|session| session.global_file_id().to_string())
}

pub fn preview_session_metadata(
    registry: &PreviewRuntimeRegistry,
    case_id: &CaseId,
    handle_id: &str,
) -> Result<(u64, Option<String>), FileServiceError> {
    registry
        .get_session(&case_id.0, handle_id)
        .map(|session| (session.size(), session.mime().map(str::to_string)))
}

pub fn invalidate_preview_source(
    registry: &PreviewRuntimeRegistry,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
) -> Result<(), FileServiceError> {
    registry.invalidate_source(&case_id.0, &data_source_id.0)
}
