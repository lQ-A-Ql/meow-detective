use crate::{
    file_service::FileServiceError, source_db, storage_runtime::ReadOnlySourceStorageHandle,
};
use domain::{DataSourceId, EntryType, FileEntry, FileEntryId};
use persistence_source::fjall::{CatalogEntryType, FileEntryRecord, ScanDirection};
use std::collections::BTreeMap;
use std::path::Path;
use transport::{
    commands::GetFileRowsRequest,
    dto::{FileChildrenDto, FileEntryRowDto, FileRowsPageDto, FileTreeNodeDto},
};

const PAGE_SIZE: usize = 256;
const MAX_PAGE_WINDOW: usize = 8_192;
const MAX_ROOT_DIRECTORIES: usize = 8_192;

pub(super) fn open(
    case_root: &Path,
    source_id: &DataSourceId,
) -> Result<Option<ReadOnlySourceStorageHandle>, FileServiceError> {
    let source = source_db::open_registered_fjall_source_read_only(case_root, source_id).map_err(
        |error| match error {
            crate::storage_runtime::StorageRuntimeError::MissingRoot(_) => {
                FileServiceError::integrity("source detail storage is missing")
            }
            crate::storage_runtime::StorageRuntimeError::Control(
                persistence_sqlite::DbError::Io(ref io),
            )
            | crate::storage_runtime::StorageRuntimeError::Source(
                persistence_source::fjall::FjallStorageError::Io(ref io),
            ) if io.kind() == std::io::ErrorKind::NotFound => {
                FileServiceError::integrity("source detail storage is missing")
            }
            other => FileServiceError::other(other.to_string()),
        },
    )?;
    let snapshot = source.snapshot();
    source
        .require_projection_complete(&snapshot, &source_id.0)
        .map_err(|error| FileServiceError::invalid_input(error.to_string()))?;
    Ok(Some(source))
}

pub(super) fn source_context(
    conn: &rusqlite::Connection,
) -> Option<(std::path::PathBuf, DataSourceId)> {
    let case_root = persistence_sqlite::connection::case_root_from_connection(conn)
        .ok()
        .flatten()?;
    let source_id = conn
        .query_row(
            "SELECT id FROM data_sources ORDER BY id LIMIT 2",
            [],
            |row| row.get::<_, String>(0),
        )
        .ok()?;
    Some((case_root, DataSourceId(source_id)))
}

pub(super) fn entry(
    case_root: &Path,
    source_id: &DataSourceId,
    file_id: &str,
) -> Result<Option<FileEntry>, FileServiceError> {
    let Some(source) = open(case_root, source_id)? else {
        return Ok(None);
    };
    let snapshot = source.snapshot();
    source
        .catalog_get(&snapshot, &source_id.0, file_id)
        .map(|record| record.map(record_to_domain))
        .map_err(|error| FileServiceError::other(error.to_string()))
}

pub fn record_to_domain(record: FileEntryRecord) -> FileEntry {
    FileEntry {
        id: FileEntryId(record.id),
        parent_id: record.parent_id.map(FileEntryId),
        data_source_id: DataSourceId(record.data_source_id),
        path: record.path,
        name: record.name,
        entry_type: match record.entry_type {
            CatalogEntryType::File => EntryType::File,
            CatalogEntryType::Directory => EntryType::Directory,
        },
        size: record.size,
        ext: record.ext,
        deleted: record.deleted,
        hidden: record.hidden,
        system: record.system,
        encrypted: record.encrypted.unwrap_or(true),
        read_only: record.read_only,
        archive: record.archive,
        unix_mode: record.unix_mode,
        created_at: record.created,
        modified_at: record.modified,
        accessed_at: record.accessed,
        changed_at: record.changed,
        hash_sha256: record.hash_sha256,
    }
}

fn checked_page_window(offset: u64, limit: u32) -> Result<usize, FileServiceError> {
    let window = offset
        .checked_add(u64::from(limit))
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| {
            FileServiceError::invalid_input("file page offset exceeds the bounded catalog window")
        })?;
    if window > MAX_PAGE_WINDOW {
        return Err(FileServiceError::invalid_input(format!(
            "file page offset exceeds the bounded catalog window of {MAX_PAGE_WINDOW} rows"
        )));
    }
    Ok(window)
}

pub(super) fn all(
    source: &ReadOnlySourceStorageHandle,
    source_id: &str,
    parent: Option<&str>,
) -> Result<Vec<FileEntry>, FileServiceError> {
    let snapshot = source.snapshot();
    let mut cursor = None;
    let mut entries = Vec::with_capacity(PAGE_SIZE);
    loop {
        let page = source
            .catalog_list_by_parent(
                &snapshot,
                source_id,
                parent,
                cursor.as_deref(),
                ScanDirection::Forward,
                PAGE_SIZE,
            )
            .map_err(|error| FileServiceError::other(error.to_string()))?;
        entries.extend(page.records.into_iter().map(record_to_domain));
        if entries.len() > MAX_PAGE_WINDOW {
            return Err(FileServiceError::invalid_input(format!(
                "file jump context exceeds the bounded catalog window of {MAX_PAGE_WINDOW} rows"
            )));
        }
        cursor = page.next_cursor;
        if cursor.is_none() {
            return Ok(entries);
        }
    }
}

fn root_catalog(
    source: &ReadOnlySourceStorageHandle,
    source_id: &str,
) -> Result<Vec<FileEntry>, FileServiceError> {
    let snapshot = source.snapshot();
    let mut by_id = BTreeMap::new();
    let mut cursor = None;
    loop {
        let page = source
            .catalog_list_root_directories(
                &snapshot,
                source_id,
                cursor.as_deref(),
                ScanDirection::Forward,
                PAGE_SIZE,
            )
            .map_err(|error| FileServiceError::other(error.to_string()))?;
        for record in page.records {
            by_id.insert(record.id.clone(), record_to_domain(record));
            if by_id.len() > MAX_ROOT_DIRECTORIES {
                return Err(FileServiceError::invalid_input(format!(
                    "root directory count exceeds the bounded limit of {MAX_ROOT_DIRECTORIES}"
                )));
            }
        }
        cursor = page.next_cursor;
        if cursor.is_none() {
            break;
        }
    }
    let mut orphan_cursor = None;
    loop {
        let page = source
            .catalog_list_orphan_directories(
                &snapshot,
                source_id,
                orphan_cursor.as_deref(),
                ScanDirection::Forward,
                PAGE_SIZE,
            )
            .map_err(|error| FileServiceError::other(error.to_string()))?;
        for record in page.records {
            let entry = record_to_domain(record);
            if let Some(parent) = entry.parent_id.as_ref() {
                if parent != &entry.id
                    && source
                        .catalog_get(&snapshot, source_id, &parent.0)
                        .map_err(|error| FileServiceError::other(error.to_string()))?
                        .is_some()
                {
                    continue;
                }
            }
            by_id.insert(entry.id.0.clone(), entry);
            if by_id.len() > MAX_ROOT_DIRECTORIES {
                return Err(FileServiceError::invalid_input(format!(
                    "root directory count exceeds the bounded limit of {MAX_ROOT_DIRECTORIES}"
                )));
            }
        }
        orphan_cursor = page.next_cursor;
        if orphan_cursor.is_none() {
            break;
        }
    }
    Ok(by_id
        .into_values()
        .filter(|entry| entry.entry_type == EntryType::Directory)
        .collect())
}

pub(super) fn tree(
    conn: &rusqlite::Connection,
    case_root: &Path,
    source_id: &DataSourceId,
    show_hidden: bool,
) -> Result<Option<Vec<FileTreeNodeDto>>, FileServiceError> {
    let Some(source) = open(case_root, source_id)? else {
        return Ok(None);
    };
    let mut entries = root_catalog(&source, &source_id.0)?
        .into_iter()
        .filter(|entry| show_hidden || (!entry.hidden && !entry.system))
        .collect::<Vec<_>>();
    super::metadata::sorting::sort_directories_for_tree(&mut entries);
    let partitions = crate::partition_query_service::PartitionQueryRepo::new(conn)
        .find_by_data_source(&source_id.0)
        .map_err(|error| FileServiceError::other(error.to_string()))?;
    let repository = persistence_source::repositories::file_repo::FileRepo::new(conn);
    let child_counts = repository
        .has_child_directories_batch_visible(
            &entries.iter().map(|entry| &entry.id).collect::<Vec<_>>(),
            show_hidden,
        )
        .map_err(|error| FileServiceError::other(error.to_string()))?;
    let mark_as_partition = partitions
        .iter()
        .any(|partition| partition.status != "redirected");
    Ok(Some(
        entries
            .iter()
            .map(|entry| {
                if super::partition_roots::looks_like_raw_fs_root_name(&entry.name) {
                    let mut normalized = entry.clone();
                    normalized.name =
                        super::partition_roots::normalized_bare_root_name_from_partitions(
                            entry,
                            &partitions,
                        );
                    super::metadata::lookup::file_entry_to_tree_node_with_partition_hint(
                        &normalized,
                        0,
                        Some(child_counts.get(&entry.id.0).copied().unwrap_or(false)),
                        mark_as_partition,
                    )
                } else {
                    super::metadata::lookup::file_entry_to_tree_node_with_partition_hint(
                        entry,
                        0,
                        Some(child_counts.get(&entry.id.0).copied().unwrap_or(false)),
                        mark_as_partition,
                    )
                }
            })
            .collect(),
    ))
}

pub(super) fn children(
    conn: &rusqlite::Connection,
    case_root: &Path,
    source_id: &DataSourceId,
    parent_id: &str,
    offset: u64,
    limit: u32,
    show_hidden: bool,
) -> Result<Option<FileChildrenDto>, FileServiceError> {
    let Some(source) = open(case_root, source_id)? else {
        return Ok(None);
    };
    checked_page_window(offset, limit)?;
    let request = GetFileRowsRequest {
        parent_id: Some(parent_id.into()),
        offset,
        limit,
        show_hidden,
        ..Default::default()
    };
    let (entries, total) = super::catalog_browse::page(&source, &source_id.0, &request, true)?;
    let visible = entries.as_slice();
    let repository = persistence_source::repositories::file_repo::FileRepo::new(conn);
    let child_counts = repository
        .has_child_directories_batch_visible(
            &visible.iter().map(|entry| &entry.id).collect::<Vec<_>>(),
            show_hidden,
        )
        .map_err(|error| FileServiceError::other(error.to_string()))?;
    Ok(Some(FileChildrenDto {
        children: visible
            .iter()
            .map(|entry| {
                super::metadata::lookup::file_entry_to_tree_node(
                    entry,
                    1,
                    Some(child_counts.get(&entry.id.0).copied().unwrap_or(false)),
                )
            })
            .collect(),
        total_count: total,
        offset: Some(offset),
        limit: Some(limit),
        truncated: Some(offset.saturating_add(limit as u64) < total),
    }))
}

pub(super) fn rows(
    case_root: &Path,
    source_id: &DataSourceId,
    request: &GetFileRowsRequest,
) -> Result<Option<FileRowsPageDto>, FileServiceError> {
    let Some(source) = open(case_root, source_id)? else {
        return Ok(None);
    };
    let mut request = request.clone();
    request.validate().map_err(FileServiceError::InvalidInput)?;
    checked_page_window(request.offset, request.limit)?;
    let (entries, total) = super::catalog_browse::page(&source, &source_id.0, &request, false)?;
    Ok(Some(FileRowsPageDto {
        rows: entries
            .iter()
            .map(super::metadata::lookup::file_entry_to_dto)
            .collect::<Vec<FileEntryRowDto>>(),
        total_count: total,
        offset: request.offset,
        limit: request.limit,
        truncated: request.offset.saturating_add(request.limit as u64) < total,
    }))
}
