use std::path::Path;

use domain::CaseId;
use rusqlite::Connection;

use crate::{
    file_service::{
        viewer::{
            image_preview_for_file, media_preview_plan_for_file, media_range_for_file,
            read_preview_bytes_for_file, text_preview_for_file, MediaPreviewPlan,
        },
        FileServiceError,
    },
    source_db::GlobalFileId,
};

use super::shared::{open_source_for_file_id, scoped_context};

pub fn text_preview_for_source_case(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    file_id: &str,
    max_bytes: Option<usize>,
) -> Result<transport::dto::TextPreviewDto, FileServiceError> {
    let (global_id, source_conn) = open_source_for_file_id(case_conn, case_root, case_id, file_id)?;
    text_preview_for_file(
        scoped_context(
            &source_conn,
            case_conn,
            case_root,
            case_id,
            &global_id.data_source_id,
        ),
        &global_id.local_id.0,
        max_bytes,
    )
}

pub fn image_preview_for_source_case(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    file_id: &str,
) -> Result<transport::dto::ImagePreviewDto, FileServiceError> {
    let (global_id, source_conn) = open_source_for_file_id(case_conn, case_root, case_id, file_id)?;
    image_preview_for_file(
        scoped_context(
            &source_conn,
            case_conn,
            case_root,
            case_id,
            &global_id.data_source_id,
        ),
        &global_id.local_id.0,
    )
}

/// Structured preview for document-like files (PDF, Office Open XML, SQLite).
pub fn document_preview_for_source_case(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    file_id: &str,
) -> Result<transport::dto::DocumentPreviewDto, FileServiceError> {
    let (global_id, source_conn) = open_source_for_file_id(case_conn, case_root, case_id, file_id)?;
    crate::file_service::viewer::document_preview_for_file(
        scoped_context(
            &source_conn,
            case_conn,
            case_root,
            case_id,
            &global_id.data_source_id,
        ),
        &global_id.local_id.0,
    )
}

pub fn media_preview_plan_for_source_case(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    file_id: &str,
) -> Result<MediaPreviewPlan, FileServiceError> {
    let (global_id, source_conn) = open_source_for_file_id(case_conn, case_root, case_id, file_id)?;
    let local_file_id = global_id.local_id.0.clone();
    let global_file_id =
        GlobalFileId::new(global_id.data_source_id.clone(), global_id.local_id.clone())
            .encode()
            .0;
    let mut plan = media_preview_plan_for_file(
        scoped_context(
            &source_conn,
            case_conn,
            case_root,
            case_id,
            &global_id.data_source_id,
        ),
        &local_file_id,
    )?;
    if let MediaPreviewPlan::Inline(dto) = &mut plan {
        dto.handle_id = Some(format!("file:{global_file_id}"));
    }
    Ok(plan)
}

pub fn media_range_for_source_case(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    file_id: &str,
    request: &transport::dto::MediaRangeRequestDto,
) -> Result<transport::dto::MediaRangeResponseDto, FileServiceError> {
    let (global_id, source_conn) = open_source_for_file_id(case_conn, case_root, case_id, file_id)?;
    media_range_for_file(
        scoped_context(
            &source_conn,
            case_conn,
            case_root,
            case_id,
            &global_id.data_source_id,
        ),
        &global_id.local_id.0,
        request,
    )
}

pub fn read_preview_bytes_for_source_case(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    file_id: &str,
    offset: u64,
    length: u32,
) -> Result<Vec<u8>, FileServiceError> {
    let (global_id, source_conn) = open_source_for_file_id(case_conn, case_root, case_id, file_id)?;
    read_preview_bytes_for_file(
        scoped_context(
            &source_conn,
            case_conn,
            case_root,
            case_id,
            &global_id.data_source_id,
        ),
        &global_id.local_id.0,
        offset,
        length,
    )
}
