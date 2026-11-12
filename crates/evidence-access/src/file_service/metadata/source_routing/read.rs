use std::path::Path;

use domain::CaseId;
use rusqlite::Connection;
use transport::dto::{ViewerHandleDto, ViewerRangeRequestDto, ViewerRangeResponseDto};

use crate::{
    file_service::{
        viewer::{file_id_from_handle, open_file_handle_real, read_file_range_for_case},
        FileServiceError,
    },
    source_db::GlobalFileId,
};

use super::shared::{open_source_for_file_id, scoped_context};

pub fn open_file_handle_for_case(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    file_id: &str,
) -> Result<ViewerHandleDto, FileServiceError> {
    let (global_id, source_conn) = open_source_for_file_id(case_conn, case_root, case_id, file_id)?;
    let mut handle = open_file_handle_real(
        scoped_context(
            &source_conn,
            case_conn,
            case_root,
            case_id,
            &global_id.data_source_id,
        ),
        &global_id.local_id.0,
    )?;
    handle.handle_id = format!(
        "file:{}",
        GlobalFileId::new(global_id.data_source_id, global_id.local_id)
            .encode()
            .0
    );
    Ok(handle)
}

pub fn read_file_range_for_source_case(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    request: &ViewerRangeRequestDto,
) -> Result<ViewerRangeResponseDto, FileServiceError> {
    let file_id = file_id_from_handle(&request.handle_id)?;
    let (global_id, source_conn) = open_source_for_file_id(case_conn, case_root, case_id, file_id)?;
    let mut local_request = request.clone();
    local_request.handle_id = format!("file:{}", global_id.local_id.0);
    read_file_range_for_case(
        scoped_context(
            &source_conn,
            case_conn,
            case_root,
            case_id,
            &global_id.data_source_id,
        ),
        &local_request,
    )
}
