use std::{path::Path, sync::Arc};

use domain::{CaseId, DataSourceId};
use transport::dto::{
    DocumentPreviewDto, ImagePreviewDto, TextPreviewDto, ViewerRangeRequestDto,
    ViewerRangeResponseDto,
};

use crate::{
    bitlocker_runtime::BitLockerUnlockRegistry,
    file_service::{
        image_preview_for_file, read_file_range_for_case, text_preview_for_file,
        viewer::document_preview_for_file, FileServiceError, SourceReadContext,
    },
    source_db::GlobalFileId,
};

use super::source_routing::open_source_for_file_id;

fn context<'a>(
    source_conn: &'a rusqlite::Connection,
    case_conn: &'a rusqlite::Connection,
    case_root: &'a Path,
    case_id: &'a CaseId,
    data_source_id: &'a DataSourceId,
    bitlocker_runtime: &Arc<BitLockerUnlockRegistry>,
) -> SourceReadContext<'a> {
    SourceReadContext::new(source_conn, case_conn, case_root, case_id, data_source_id)
        .with_bitlocker_runtime(bitlocker_runtime.clone())
}

pub fn read_file_range_for_source_case_with_bitlocker(
    bitlocker_runtime: &Arc<BitLockerUnlockRegistry>,
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    case_id: &CaseId,
    request: &ViewerRangeRequestDto,
) -> Result<ViewerRangeResponseDto, FileServiceError> {
    let file_id = crate::file_service::viewer::file_id_from_handle(&request.handle_id)?;
    let (global_id, source_conn) = open_source_for_file_id(case_conn, case_root, case_id, file_id)?;
    let mut local_request = request.clone();
    local_request.handle_id = format!("file:{}", global_id.local_id.0);
    read_file_range_for_case(
        context(
            &source_conn,
            case_conn,
            case_root,
            case_id,
            &global_id.data_source_id,
            bitlocker_runtime,
        ),
        &local_request,
    )
}

pub fn text_preview_for_source_case_with_bitlocker(
    bitlocker_runtime: &Arc<BitLockerUnlockRegistry>,
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    case_id: &CaseId,
    file_id: &str,
    max_bytes: Option<usize>,
) -> Result<TextPreviewDto, FileServiceError> {
    let (global_id, source_conn) = open_source_for_file_id(case_conn, case_root, case_id, file_id)?;
    text_preview_for_file(
        context(
            &source_conn,
            case_conn,
            case_root,
            case_id,
            &global_id.data_source_id,
            bitlocker_runtime,
        ),
        &global_id.local_id.0,
        max_bytes,
    )
}

pub fn image_preview_for_source_case_with_bitlocker(
    bitlocker_runtime: &Arc<BitLockerUnlockRegistry>,
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    case_id: &CaseId,
    file_id: &str,
) -> Result<ImagePreviewDto, FileServiceError> {
    let (global_id, source_conn) = open_source_for_file_id(case_conn, case_root, case_id, file_id)?;
    image_preview_for_file(
        context(
            &source_conn,
            case_conn,
            case_root,
            case_id,
            &global_id.data_source_id,
            bitlocker_runtime,
        ),
        &global_id.local_id.0,
    )
}

pub fn document_preview_for_source_case_with_bitlocker(
    bitlocker_runtime: &Arc<BitLockerUnlockRegistry>,
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    case_id: &CaseId,
    file_id: &str,
) -> Result<DocumentPreviewDto, FileServiceError> {
    let (global_id, source_conn) = open_source_for_file_id(case_conn, case_root, case_id, file_id)?;
    document_preview_for_file(
        context(
            &source_conn,
            case_conn,
            case_root,
            case_id,
            &global_id.data_source_id,
            bitlocker_runtime,
        ),
        &global_id.local_id.0,
    )
}

pub fn media_preview_plan_for_source_case_with_bitlocker(
    bitlocker_runtime: &Arc<BitLockerUnlockRegistry>,
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    case_id: &CaseId,
    file_id: &str,
) -> Result<crate::file_service::MediaPreviewPlan, FileServiceError> {
    let (global_id, source_conn) = open_source_for_file_id(case_conn, case_root, case_id, file_id)?;
    let local_file_id = global_id.local_id.0.clone();
    let global_file_id =
        GlobalFileId::new(global_id.data_source_id.clone(), global_id.local_id.clone())
            .encode()
            .0;
    let mut plan = crate::file_service::media_preview_plan_for_file(
        context(
            &source_conn,
            case_conn,
            case_root,
            case_id,
            &global_id.data_source_id,
            bitlocker_runtime,
        ),
        &local_file_id,
    )?;
    if let crate::file_service::MediaPreviewPlan::Inline(dto) = &mut plan {
        dto.handle_id = Some(format!("file:{global_file_id}"));
    }
    Ok(plan)
}
