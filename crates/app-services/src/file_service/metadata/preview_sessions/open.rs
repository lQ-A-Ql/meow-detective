use std::{path::Path, sync::Arc};

use domain::{CaseId, DataSourceId};
use transport::dto::ViewerHandleDto;

use crate::{
    ceph_reconstruction::open_cephfs_file_reader,
    file_service::{
        metadata::source_routing::open_source_for_file_id,
        preview_runtime::{PreparedFile, PreviewScopeToken, PreviewSession},
        viewer::{
            detect_preview_for_file, exact_partition_candidate, mft_file_locator_from_entry_id,
            open_filesystem_reader, preview_descriptor_for_case, DetectedPreview,
            PreviewDescriptor, PreviewReadContext,
        },
        FileServiceError, PreviewRuntimeRegistry, SourceReadContext,
    },
    source_db::GlobalFileId,
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

struct OpenedPreview {
    global_id: GlobalFileId,
    descriptor: PreviewDescriptor,
    detected: Option<DetectedPreview>,
    effective_mime: Option<String>,
    global_file_id: String,
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
    let opened = resolve_opened_preview(
        &source_conn,
        case_conn,
        case_root,
        case_id,
        global_id,
        bitlocker_runtime,
    )?;
    let prepared_file = build_prepared_file(
        bitlocker_runtime,
        &source_conn,
        case_conn,
        case_root,
        case_id,
        &opened,
    )?;
    let session = build_preview_session(
        SessionBuildContext {
            registry,
            scope: &scope,
            source_conn: &source_conn,
            case_conn,
            case_root,
            case_id,
            opened: &opened,
        },
        prepared_file,
    )?;
    let handle_id = registry.insert_session(&scope, session)?;
    Ok(ViewerHandleDto {
        handle_id,
        size: opened.descriptor.size,
        mime: opened.effective_mime,
        preview_kind: opened.detected.as_ref().map(|value| value.kind),
    })
}

fn resolve_opened_preview(
    source_conn: &rusqlite::Connection,
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    case_id: &CaseId,
    global_id: GlobalFileId,
    bitlocker_runtime: Option<&Arc<crate::bitlocker_runtime::BitLockerUnlockRegistry>>,
) -> Result<OpenedPreview, FileServiceError> {
    let descriptor = preview_descriptor_for_case(source_conn, &case_id.0, &global_id.local_id)?;
    let detected = detect_preview(
        source_conn,
        case_conn,
        case_root,
        case_id,
        &global_id,
        bitlocker_runtime,
    );
    let effective_mime = detected
        .as_ref()
        .map(|value| value.mime.to_string())
        .or_else(|| descriptor.mime.clone());
    let global_file_id =
        GlobalFileId::new(global_id.data_source_id.clone(), global_id.local_id.clone())
            .encode()
            .0;
    Ok(OpenedPreview {
        global_id,
        descriptor,
        detected,
        effective_mime,
        global_file_id,
    })
}

fn detect_preview(
    source_conn: &rusqlite::Connection,
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    case_id: &CaseId,
    global_id: &GlobalFileId,
    bitlocker_runtime: Option<&Arc<crate::bitlocker_runtime::BitLockerUnlockRegistry>>,
) -> Option<DetectedPreview> {
    let context = SourceReadContext::new(
        source_conn,
        case_conn,
        case_root,
        case_id,
        &global_id.data_source_id,
    );
    let mut context = match bitlocker_runtime {
        Some(runtime) => context.with_bitlocker_runtime(runtime.clone()),
        None => context,
    };
    detect_preview_for_file(&mut context, &global_id.local_id.0)
        .ok()
        .flatten()
}

fn build_prepared_file(
    bitlocker_runtime: Option<&Arc<crate::bitlocker_runtime::BitLockerUnlockRegistry>>,
    source_conn: &rusqlite::Connection,
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    case_id: &CaseId,
    opened: &OpenedPreview,
) -> Result<Option<PreparedFile>, FileServiceError> {
    if opened.descriptor.size == 0 {
        return Ok(None);
    }
    prepare_local_preview_file(
        bitlocker_runtime,
        source_conn,
        case_conn,
        case_root,
        case_id,
        &opened.global_id.data_source_id,
        &opened.descriptor,
    )
}

struct SessionBuildContext<'a, 'registry> {
    registry: &'a PreviewRuntimeRegistry,
    scope: &'a PreviewScopeToken<'registry>,
    source_conn: &'a rusqlite::Connection,
    case_conn: &'a rusqlite::Connection,
    case_root: &'a Path,
    case_id: &'a CaseId,
    opened: &'a OpenedPreview,
}

fn build_preview_session(
    context: SessionBuildContext<'_, '_>,
    prepared_file: Option<PreparedFile>,
) -> Result<PreviewSession, FileServiceError> {
    let SessionBuildContext {
        registry,
        scope,
        source_conn,
        case_conn,
        case_root,
        case_id,
        opened,
    } = context;
    let descriptor = &opened.descriptor;
    if descriptor.size == 0 {
        return Ok(PreviewSession::routed(
            case_id.0.clone(),
            opened.global_id.data_source_id.0.clone(),
            opened.global_file_id.clone(),
            descriptor.size,
            opened.effective_mime.clone(),
        ));
    }
    if let Some(file) = prepared_file {
        return Ok(PreviewSession::prepared_file(
            case_id.0.clone(),
            opened.global_id.data_source_id.0.clone(),
            opened.global_file_id.clone(),
            descriptor.size,
            opened.effective_mime.clone(),
            file,
        ));
    }
    if descriptor.source_kind == "ceph_rbd" {
        return build_ceph_rbd_session(
            registry,
            scope,
            source_conn,
            case_conn,
            case_root,
            case_id,
            opened,
        );
    }
    if descriptor.source_kind == "ceph_fs" {
        return build_ceph_fs_session(case_conn, case_root, case_id, opened);
    }
    Ok(PreviewSession::routed(
        case_id.0.clone(),
        opened.global_id.data_source_id.0.clone(),
        opened.global_file_id.clone(),
        descriptor.size,
        opened.effective_mime.clone(),
    ))
}

fn build_ceph_rbd_session(
    registry: &PreviewRuntimeRegistry,
    scope: &PreviewScopeToken<'_>,
    source_conn: &rusqlite::Connection,
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    case_id: &CaseId,
    opened: &OpenedPreview,
) -> Result<PreviewSession, FileServiceError> {
    let descriptor = &opened.descriptor;
    let runtime = registry.resolve_derived_runtime(
        case_conn,
        case_root,
        case_id,
        &opened.global_id.data_source_id,
        scope,
    )?;
    let candidate = exact_partition_candidate(descriptor)?;
    let filesystem = registry.resolve_derived_filesystem(
        source_conn,
        &opened.global_id.data_source_id,
        &runtime,
        candidate,
        scope,
    )?;
    PreviewSession::prepared_ceph(
        case_id.0.clone(),
        opened.global_file_id.clone(),
        descriptor.size,
        opened.effective_mime.clone(),
        runtime.lineage_fingerprint().to_string(),
        filesystem,
        descriptor,
    )
}

fn build_ceph_fs_session(
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    case_id: &CaseId,
    opened: &OpenedPreview,
) -> Result<PreviewSession, FileServiceError> {
    let descriptor = &opened.descriptor;
    let reader = open_cephfs_file_reader(
        case_conn,
        case_root,
        case_id,
        &opened.global_id.data_source_id,
        &crate::file_service::cephfs_adapter::file_read_request(descriptor)?,
    )?;
    Ok(PreviewSession::prepared_cephfs(
        case_id.0.clone(),
        opened.global_file_id.clone(),
        descriptor.size,
        opened.effective_mime.clone(),
        descriptor,
        reader,
    ))
}

fn prepare_local_preview_file(
    bitlocker_runtime: Option<&Arc<crate::bitlocker_runtime::BitLockerUnlockRegistry>>,
    source_conn: &rusqlite::Connection,
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
    descriptor: &PreviewDescriptor,
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
    descriptor: &PreviewDescriptor,
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
    descriptor: &PreviewDescriptor,
) -> Result<Option<PreparedFile>, FileServiceError> {
    if !matches!(
        descriptor.source_kind.as_str(),
        "e01" | "raw" | "local_disk"
    ) {
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
