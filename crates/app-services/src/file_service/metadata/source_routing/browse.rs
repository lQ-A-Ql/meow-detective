use std::path::Path;

use domain::{CaseId, DataSourceId, FileEntryId};
use rusqlite::Connection;
use transport::{
    commands::{GetFileJumpContextRequest, GetFileRowsRequest},
    dto::{FileChildrenDto, FileJumpContextDto, FileRowsPageDto, FileTreeNodeDto},
};

use crate::{
    file_service::{
        browse::{get_file_children_lazy_with_visibility, get_file_rows_for_request},
        FileServiceError,
    },
    source_db::GlobalFileId,
};

use super::shared::open_source_for_file_id;

fn wrap_tree_nodes(nodes: &mut [FileTreeNodeDto]) {
    for node in nodes {
        if let Some(data_source_id) = &node.data_source_id {
            if !node.id.starts_with("ds:") {
                node.id = GlobalFileId::new(
                    DataSourceId(data_source_id.clone()),
                    FileEntryId(node.id.clone()),
                )
                .encode()
                .0;
            }
        }
    }
}

fn wrap_jump_context(context: &mut FileJumpContextDto, data_source_id: &DataSourceId) {
    wrap_row_id(&mut context.target, data_source_id);
    wrap_row_id(&mut context.directory, data_source_id);
    for id in &mut context.ancestor_directory_ids {
        if !id.starts_with("ds:") {
            *id = GlobalFileId::new(data_source_id.clone(), FileEntryId(id.clone()))
                .encode()
                .0;
        }
    }
}

fn wrap_row_id(row: &mut transport::dto::FileEntryRowDto, data_source_id: &DataSourceId) {
    if !row.id.starts_with("ds:") {
        row.id = GlobalFileId::new(data_source_id.clone(), FileEntryId(row.id.clone()))
            .encode()
            .0;
    }
    if let Some(parent_id) = row.parent_id.clone() {
        if !parent_id.starts_with("ds:") {
            row.parent_id = Some(
                GlobalFileId::new(data_source_id.clone(), FileEntryId(parent_id))
                    .encode()
                    .0,
            );
        }
    }
}

pub fn get_file_tree_for_case(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &domain::CaseId,
    show_hidden: bool,
) -> Result<Vec<FileTreeNodeDto>, FileServiceError> {
    let mut roots = Vec::new();
    for (_, source_conn) in
        crate::source_db::open_ready_source_connections_read_only(case_conn, case_root, case_id)?
    {
        let mut nodes =
            crate::file_service::get_file_tree_real_with_visibility(&source_conn, show_hidden)?;
        wrap_tree_nodes(&mut nodes);
        roots.extend(nodes);
    }
    Ok(roots)
}

pub fn get_file_children_for_case(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    parent_id: &str,
    offset: u64,
    limit: u32,
    show_hidden: bool,
) -> Result<FileChildrenDto, FileServiceError> {
    let (global_id, source_conn) =
        open_source_for_file_id(case_conn, case_root, case_id, parent_id)?;
    let mut children = get_file_children_lazy_with_visibility(
        &source_conn,
        &global_id.local_id.0,
        offset,
        limit,
        show_hidden,
    )?;
    wrap_tree_nodes(&mut children.children);
    Ok(children)
}

pub fn get_file_rows_for_case(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    request: &GetFileRowsRequest,
) -> Result<FileRowsPageDto, FileServiceError> {
    let Some(parent_id) = request.parent_id.as_deref() else {
        return Ok(FileRowsPageDto {
            rows: Vec::new(),
            total_count: 0,
            offset: request.offset,
            limit: request.limit,
            truncated: false,
        });
    };
    let (global_id, source_conn) =
        open_source_for_file_id(case_conn, case_root, case_id, parent_id)?;
    let mut local_request = request.clone();
    local_request.parent_id = Some(global_id.local_id.0);
    let mut page = get_file_rows_for_request(&source_conn, &local_request)?;
    for row in &mut page.rows {
        wrap_row_id(row, &global_id.data_source_id);
    }
    Ok(page)
}

pub fn get_file_jump_context_for_case(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    request: &GetFileJumpContextRequest,
) -> Result<FileJumpContextDto, FileServiceError> {
    let (global_id, source_conn) =
        open_source_for_file_id(case_conn, case_root, case_id, &request.file_id)?;
    let mut local_request = request.clone();
    local_request.file_id = global_id.local_id.0.clone();
    let mut context = crate::file_service::get_file_jump_context(&source_conn, &local_request)?;
    wrap_jump_context(&mut context, &global_id.data_source_id);
    Ok(context)
}
