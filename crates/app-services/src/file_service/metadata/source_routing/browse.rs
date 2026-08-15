use std::path::Path;

use domain::{CaseId, DataSourceId, FileEntryId};
use persistence_sqlite::repositories::{
    datasource_repo::DataSourceRepo, file_repo::FileRepo, partition_repo::PartitionRepo,
};
use rusqlite::Connection;
use transport::{
    commands::{GetFileJumpContextRequest, GetFileRowsRequest},
    dto::{
        DataSourcePartitionDto, DataSourceSummaryDto, FileChildrenDto, FileJumpContextDto,
        FileRowsPageDto, FileTreeNodeDto,
    },
};

use crate::{
    file_service::{
        browse::{get_file_children_lazy_with_visibility, get_file_rows_for_request},
        data_sources::{data_source_hash_status_label, data_source_provenance_status_label},
        FileServiceError,
    },
    source_db::GlobalFileId,
};

use super::shared::{open_source_for_data_source, open_source_for_file_id};

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

pub fn get_data_sources_for_case(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &domain::CaseId,
) -> Result<Vec<DataSourceSummaryDto>, FileServiceError> {
    let sources = DataSourceRepo::new(case_conn).find_by_case(case_id)?;
    let mut summaries = Vec::with_capacity(sources.len());

    for source in sources {
        let storage = DataSourceRepo::new(case_conn)
            .find_storage(&source.id)?
            .ok_or_else(|| {
                FileServiceError::other(format!(
                    "data source {} is missing storage metadata",
                    source.id.0
                ))
            })?;
        let platform = crate::file_service::data_sources::required_data_source_platform(&storage)?;
        let processing = crate::processing_phase_service::get_data_source_processing_summary(
            case_conn, &source.id,
        )?;
        let source_conn =
            open_source_for_data_source(case_conn, case_root, case_id, &source.id).ok();
        let (file_count, partitions) = if let Some(source_conn) = source_conn.as_ref() {
            let file_count = FileRepo::new(source_conn)
                .count_by_data_source(&source.id)
                .ok();
            let partitions = PartitionRepo::new(source_conn)
                .find_by_data_source(&source.id.0)
                .map(|items| {
                    items
                        .into_iter()
                        .map(|item| DataSourcePartitionDto {
                            index: item.partition_index,
                            name: item.name,
                            kind_label: item.kind_label,
                            status: item.status,
                            offset: item.offset,
                            length: item.length,
                            type_guid: item.type_guid,
                            filesystem: item.filesystem,
                            unlock_hint: item.unlock_hint,
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            (file_count, partitions)
        } else {
            (None, Vec::new())
        };

        summaries.push(DataSourceSummaryDto {
            id: source.id.0.clone(),
            name: source.name,
            kind: source.kind.to_string(),
            source_path: source.source_path.display().to_string(),
            imported_at: source.imported_at.to_rfc3339(),
            file_count,
            storage_model: Some(storage.storage_model),
            source_db_rel_path: storage.source_db_rel_path,
            index_rel_path: storage.index_rel_path,
            staging_rel_path: storage.staging_rel_path,
            platform,
            profile: storage.profile,
            import_state: Some(storage.import_state),
            schema_version: storage.schema_version,
            last_error: storage.last_error,
            processing,
            source_hash: source.provenance.source_hash_sha256,
            hash_status: Some(data_source_hash_status_label(
                &source.provenance.hash_status,
            )),
            canonical_path: source
                .provenance
                .canonical_source_path
                .map(|path| path.display().to_string()),
            evidence_size: source.provenance.evidence_size,
            reader_kind: source.provenance.reader_kind,
            provenance_status: Some(data_source_provenance_status_label(
                &source.provenance.provenance_status,
            )),
            warnings: source.provenance.warnings,
            partitions,
        });
    }

    Ok(summaries)
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
