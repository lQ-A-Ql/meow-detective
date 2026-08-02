use std::{path::Path, sync::Arc};

use base64::Engine;
use domain::{CaseId, DataSourceId};
use transport::dto::{
    MediaRangeRequestDto, MediaRangeResponseDto, ViewerHandleDto, ViewerRangeRequestDto,
    ViewerRangeResponseDto,
};

use crate::{
    ceph_reconstruction::open_cephfs_file_reader,
    file_service::{
        metadata::source_routing::{open_source_for_file_id, read_file_range_for_source_case},
        preview_runtime::{PreparedFile, PreviewSession},
        viewer::{
            exact_partition_candidate, mft_file_locator_from_entry_id, open_filesystem_reader,
            preview_descriptor_for_case, PreviewReadContext,
        },
        FileServiceError, PreviewRuntimeRegistry, SourceReadContext,
    },
};

pub fn open_preview_session_for_case(
    registry: &PreviewRuntimeRegistry,
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    case_id: &CaseId,
    file_id: &str,
) -> Result<ViewerHandleDto, FileServiceError> {
    open_preview_session_internal(registry, case_conn, case_root, case_id, file_id, None)
}

pub fn open_preview_session_for_case_with_bitlocker(
    bitlocker_runtime: &Arc<crate::bitlocker_runtime::BitLockerUnlockRegistry>,
    registry: &PreviewRuntimeRegistry,
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    case_id: &CaseId,
    file_id: &str,
) -> Result<ViewerHandleDto, FileServiceError> {
    open_preview_session_internal(
        registry,
        case_conn,
        case_root,
        case_id,
        file_id,
        Some(bitlocker_runtime),
    )
}

fn open_preview_session_internal(
    registry: &PreviewRuntimeRegistry,
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    case_id: &CaseId,
    file_id: &str,
    bitlocker_runtime: Option<&Arc<crate::bitlocker_runtime::BitLockerUnlockRegistry>>,
) -> Result<ViewerHandleDto, FileServiceError> {
    let (global_id, source_conn) = open_source_for_file_id(case_conn, case_root, case_id, file_id)?;
    let scope = registry.begin_session(case_id, &global_id.data_source_id)?;
    let descriptor = preview_descriptor_for_case(&source_conn, &case_id.0, &global_id.local_id)?;
    let global_file_id =
        crate::source_db::GlobalFileId::new(global_id.data_source_id.clone(), global_id.local_id)
            .encode()
            .0;

    let prepared_file = prepare_local_preview_file(
        bitlocker_runtime,
        &source_conn,
        case_conn,
        case_root,
        case_id,
        &global_id.data_source_id,
        &descriptor,
    )?;
    let session = if let Some(file) = prepared_file {
        PreviewSession::prepared_file(
            case_id.0.clone(),
            global_id.data_source_id.0.clone(),
            global_file_id,
            descriptor.size,
            descriptor.mime.clone(),
            file,
        )
    } else if descriptor.source_kind == "ceph_rbd" {
        let runtime = registry.resolve_derived_runtime(
            case_conn,
            case_root,
            case_id,
            &global_id.data_source_id,
            &scope,
        )?;
        let candidate = exact_partition_candidate(&descriptor)?;
        let filesystem = registry.resolve_derived_filesystem(
            &source_conn,
            &global_id.data_source_id,
            &runtime,
            candidate,
            &scope,
        )?;
        PreviewSession::prepared_ceph(
            case_id.0.clone(),
            global_file_id,
            descriptor.size,
            descriptor.mime.clone(),
            runtime.lineage_fingerprint().to_string(),
            filesystem,
            &descriptor,
        )?
    } else if descriptor.source_kind == "ceph_fs" {
        let reader = open_cephfs_file_reader(
            case_conn,
            case_root,
            case_id,
            &global_id.data_source_id,
            &crate::file_service::cephfs_adapter::file_read_request(&descriptor)?,
        )?;
        PreviewSession::prepared_cephfs(
            case_id.0.clone(),
            global_file_id,
            descriptor.size,
            descriptor.mime.clone(),
            &descriptor,
            reader,
        )
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

fn prepare_local_preview_file(
    bitlocker_runtime: Option<&Arc<crate::bitlocker_runtime::BitLockerUnlockRegistry>>,
    source_conn: &rusqlite::Connection,
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
    descriptor: &crate::file_service::PreviewDescriptor,
) -> Result<Option<PreparedFile>, FileServiceError> {
    let prepared_bitlocker = prepare_bitlocker_ntfs_file(
        bitlocker_runtime,
        source_conn,
        case_conn,
        case_root,
        case_id,
        data_source_id,
        descriptor,
    )?;
    if prepared_bitlocker.is_some() {
        return Ok(prepared_bitlocker);
    }
    prepare_native_filesystem_file(
        source_conn,
        case_conn,
        case_root,
        case_id,
        data_source_id,
        descriptor,
    )
}

fn prepare_bitlocker_ntfs_file(
    runtime: Option<&Arc<crate::bitlocker_runtime::BitLockerUnlockRegistry>>,
    source_conn: &rusqlite::Connection,
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
    descriptor: &crate::file_service::PreviewDescriptor,
) -> Result<Option<PreparedFile>, FileServiceError> {
    let Some(runtime) = runtime else {
        return Ok(None);
    };
    let Some((partition_index, inode)) = mft_file_locator_from_entry_id(&descriptor.file_id) else {
        return Ok(None);
    };
    let candidate = exact_partition_candidate(descriptor)?;
    if partition_index != candidate.partition_index {
        return Err(FileServiceError::security(
            "MFT file identifier does not match the routed partition",
        ));
    }

    let mut context =
        SourceReadContext::new(source_conn, case_conn, case_root, case_id, data_source_id)
            .with_bitlocker_runtime(runtime.clone());
    if !context.is_bitlocker_candidate(candidate)? {
        return Ok(None);
    }
    let (reader, filesystem_offset, filesystem_kind) =
        context.open_candidate_block_reader(descriptor, candidate)?;
    if !filesystem_kind.eq_ignore_ascii_case("NTFS") {
        return Ok(None);
    }
    PreparedFile::open_ntfs(reader, filesystem_offset, inode).map(Some)
}

fn prepare_native_filesystem_file(
    source_conn: &rusqlite::Connection,
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
    descriptor: &crate::file_service::PreviewDescriptor,
) -> Result<Option<PreparedFile>, FileServiceError> {
    if !matches!(descriptor.source_kind.as_str(), "e01" | "raw") {
        return Ok(None);
    }
    let candidate = match exact_partition_candidate(descriptor) {
        Ok(candidate) => candidate,
        Err(_) => return Ok(None),
    };
    let mut context =
        SourceReadContext::new(source_conn, case_conn, case_root, case_id, data_source_id);
    if context.is_bitlocker_candidate(candidate)? {
        return Ok(None);
    }
    let (reader, filesystem_offset, filesystem_kind) =
        match context.open_candidate_block_reader(descriptor, candidate) {
            Ok(value) => value,
            Err(_) => return Ok(None),
        };
    if filesystem_kind.eq_ignore_ascii_case("NTFS") {
        if let Some((partition_index, inode)) = mft_file_locator_from_entry_id(&descriptor.file_id)
        {
            if partition_index != candidate.partition_index {
                return Err(FileServiceError::security(
                    "MFT file identifier does not match the routed partition",
                ));
            }
            return PreparedFile::open_ntfs(reader, filesystem_offset, inode).map(Some);
        }
    }
    let filesystem = match open_filesystem_reader(candidate, reader, filesystem_offset) {
        Ok(filesystem) => filesystem,
        Err(_) => return Ok(None),
    };
    PreparedFile::open_filesystem(filesystem, descriptor).map(Some)
}

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
    match bitlocker_runtime {
        Some(runtime) => crate::file_service::read_file_range_for_source_case_with_bitlocker(
            runtime, case_conn, case_root, case_id, &request,
        ),
        None => read_file_range_for_source_case(case_conn, case_root, case_id, &request),
    }
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
