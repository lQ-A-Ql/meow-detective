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
        FileRowsPageDto, FileTreeNodeDto, ViewerHandleDto, ViewerRangeRequestDto,
        ViewerRangeResponseDto,
    },
};

use crate::{
    file_service::{
        data_sources::{data_source_hash_status_label, data_source_provenance_status_label},
        file_rows::get_file_rows_for_request,
        preview::{
            image_preview_for_file, media_preview_plan_for_file, media_range_for_file,
            read_preview_bytes_for_file, text_preview_for_file, MediaPreviewPlan,
        },
        tree_queries::get_file_children_lazy_with_visibility,
        viewer::{file_id_from_handle, open_file_handle_real, read_file_range_for_case},
        FileServiceError,
    },
    source_db::{GlobalFileId, SourceConnectionManager},
};

fn source_manager(case_root: &Path) -> SourceConnectionManager {
    SourceConnectionManager::new(case_root.to_path_buf())
}

fn open_source_for_data_source(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
) -> Result<Connection, FileServiceError> {
    Ok(source_manager(case_root).open_ready(case_conn, case_id, data_source_id)?)
}

fn open_source_for_file_id(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    file_id: &str,
) -> Result<(GlobalFileId, Connection), FileServiceError> {
    Ok(source_manager(case_root).open_ready_for_global_file_id(
        case_conn,
        case_id,
        &FileEntryId(file_id.to_string()),
    )?)
}

type SourceScopedContext<'a> = (
    &'a Connection,
    &'a str,
    fn(&str) -> Option<serde_json::Value>,
    fn(&str, &serde_json::Value),
);

fn scoped_context<'a>(source_conn: &'a Connection, case_id: &'a str) -> SourceScopedContext<'a> {
    (source_conn, case_id, |_| None, |_, _| {})
}

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
        let platform = super::data_sources::required_data_source_platform(&storage)?;
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
        crate::source_db::open_ready_source_connections(case_conn, case_root, case_id)?
    {
        let mut nodes =
            super::tree_queries::get_file_tree_real_with_visibility(&source_conn, show_hidden)?;
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
    let mut context = super::get_file_jump_context(&source_conn, &local_request)?;
    wrap_jump_context(&mut context, &global_id.data_source_id);
    Ok(context)
}

pub fn open_file_handle_for_case(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    file_id: &str,
) -> Result<ViewerHandleDto, FileServiceError> {
    let (global_id, source_conn) = open_source_for_file_id(case_conn, case_root, case_id, file_id)?;
    let mut handle = open_file_handle_real(
        scoped_context(&source_conn, &case_id.0),
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
    read_file_range_for_case(scoped_context(&source_conn, &case_id.0), &local_request)
}

pub fn text_preview_for_source_case(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    file_id: &str,
    max_bytes: Option<usize>,
) -> Result<transport::dto::TextPreviewDto, FileServiceError> {
    let (global_id, source_conn) = open_source_for_file_id(case_conn, case_root, case_id, file_id)?;
    text_preview_for_file(
        scoped_context(&source_conn, &case_id.0),
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
        scoped_context(&source_conn, &case_id.0),
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
    let global_file_id = GlobalFileId::new(global_id.data_source_id, global_id.local_id)
        .encode()
        .0;
    let mut plan =
        media_preview_plan_for_file(scoped_context(&source_conn, &case_id.0), &local_file_id)?;
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
        scoped_context(&source_conn, &case_id.0),
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
        scoped_context(&source_conn, &case_id.0),
        &global_id.local_id.0,
        offset,
        length,
    )
}

pub fn extract_file_to_destination_for_case(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    file_id: &str,
    destination_path: &Path,
    overwrite: bool,
) -> Result<u64, FileServiceError> {
    let (global_id, source_conn) = open_source_for_file_id(case_conn, case_root, case_id, file_id)?;
    super::export::extract_file_to_destination(
        &source_conn,
        &global_id.local_id.0,
        destination_path,
        overwrite,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use persistence_sqlite::repositories::datasource_repo::{DataSourceRepo, DataSourceStorage};
    fn setup_case_conn() -> rusqlite::Connection {
        let conn = persistence_sqlite::connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE data_sources (
                id TEXT PRIMARY KEY NOT NULL,
                case_id TEXT NOT NULL,
                name TEXT NOT NULL,
                kind TEXT NOT NULL,
                source_path TEXT NOT NULL,
                imported_at TEXT NOT NULL DEFAULT (datetime('now')),
                source_hash_sha256 TEXT,
                hash_status TEXT DEFAULT 'unknown',
                canonical_source_path TEXT,
                evidence_size INTEGER,
                reader_kind TEXT,
                provenance_status TEXT DEFAULT 'unknown',
                provenance_warnings TEXT DEFAULT '[]',
                storage_model TEXT NOT NULL DEFAULT 'source_db',
                source_db_rel_path TEXT,
                index_rel_path TEXT,
                staging_rel_path TEXT,
                platform TEXT NOT NULL DEFAULT 'unknown',
                profile TEXT,
                import_state TEXT NOT NULL DEFAULT 'pending',
                schema_version TEXT,
                last_error TEXT
            );",
        )
        .unwrap();
        conn
    }

    fn insert_data_source(conn: &rusqlite::Connection, id: &str, name: &str) {
        let ds = domain::DataSource {
            id: DataSourceId(id.to_string()),
            name: name.to_string(),
            kind: domain::DataSourceKind::LogicalDirectory,
            source_path: std::path::PathBuf::from(format!("D:/{name}")),
            imported_at: chrono::Utc::now(),
            provenance: domain::DataSourceProvenance::unknown(),
        };
        DataSourceRepo::new(conn)
            .insert_with_storage(
                &domain::CaseId("case-1".to_string()),
                &ds,
                &DataSourceStorage::source_db(id, Some("linux"), None),
            )
            .unwrap();
        DataSourceRepo::new(conn)
            .update_import_state(&ds.id, "ready", None)
            .unwrap();
    }

    fn seed_source_db(case_root: &Path, data_source_id: &str) {
        let conn =
            crate::source_db::open_source_db(case_root, &DataSourceId(data_source_id.to_string()))
                .unwrap();
        conn.execute(
            "INSERT INTO file_entries
             (id, parent_id, data_source_id, path, name, entry_type, size, ext, deleted, hidden, system)
             VALUES ('file-1', NULL, ?1, '/', '/', 'directory', NULL, NULL, 0, 0, 0)",
            [data_source_id],
        )
        .unwrap();
    }

    #[test]
    fn tree_wraps_duplicate_local_ids_by_data_source() {
        let tmp = tempfile::TempDir::new().unwrap();
        let case_conn = setup_case_conn();
        insert_data_source(&case_conn, "ds-a", "Source A");
        insert_data_source(&case_conn, "ds-b", "Source B");
        seed_source_db(tmp.path(), "ds-a");
        seed_source_db(tmp.path(), "ds-b");
        let roots = get_file_tree_for_case(
            &case_conn,
            tmp.path(),
            &domain::CaseId("case-1".to_string()),
            false,
        )
        .unwrap();
        let ids = roots.into_iter().map(|node| node.id).collect::<Vec<_>>();
        assert!(ids.contains(&"ds:ds-a:file-1".to_string()));
        assert!(ids.contains(&"ds:ds-b:file-1".to_string()));
    }
}
