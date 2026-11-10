use crate::{
    file_service::FileServiceError, source_db, storage_runtime::ReadOnlySourceStorageHandle,
};
use domain::{DataSourceId, EntryType, FileEntry, FileEntryId};
use persistence_fjall::{CatalogEntryType, FileEntryRecord, ScanDirection};
use std::collections::BTreeMap;
use std::path::Path;
use transport::{
    commands::{FileSortDirectionDto, FileSortKeyDto, GetFileRowsRequest},
    dto::{FileChildrenDto, FileEntryRowDto, FileRowsPageDto, FileTreeNodeDto},
};

const PAGE_SIZE: usize = 256;
const MAX_PAGE_WINDOW: usize = 8_192;
const MAX_ROOT_DIRECTORIES: usize = 8_192;

pub(super) fn open(
    case_root: &Path,
    source_id: &DataSourceId,
) -> Result<Option<ReadOnlySourceStorageHandle>, FileServiceError> {
    let root = source_db::fjall_source_storage_root(case_root, &source_id.0)
        .map_err(|error| FileServiceError::other(error.to_string()))?;
    if !root.is_dir() {
        return Err(FileServiceError::Integrity(
            "source detail storage is missing".into(),
        ));
    }
    let source = source_db::open_registered_fjall_source_read_only(case_root, source_id)
        .map_err(|error| FileServiceError::other(error.to_string()))?;
    let snapshot = source.snapshot();
    source
        .require_projection_complete(&snapshot, &source_id.0)
        .map_err(|error| FileServiceError::invalid_input(error.to_string()))?;
    Ok(Some(source))
}

pub(super) fn source_context(
    conn: &rusqlite::Connection,
) -> Option<(std::path::PathBuf, DataSourceId)> {
    let database_path: String = conn
        .query_row("PRAGMA database_list", [], |row| row.get(2))
        .ok()?;
    let source_id = conn
        .query_row(
            "SELECT id FROM data_sources ORDER BY id LIMIT 2",
            [],
            |row| row.get::<_, String>(0),
        )
        .ok()?;
    let case_root = Path::new(&database_path)
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)?
        .to_path_buf();
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

pub(crate) fn record_to_domain(record: FileEntryRecord) -> FileEntry {
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

fn retain_sorted_page(
    entries: &mut Vec<FileEntry>,
    mut incoming: impl Iterator<Item = FileEntry>,
    window: usize,
    sort_key: FileSortKeyDto,
    sort_direction: FileSortDirectionDto,
) {
    entries.extend(incoming.by_ref());
    super::metadata::sorting::sort_entries(entries, sort_key, sort_direction);
    entries.truncate(window);
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

fn scan_rows(
    source: &ReadOnlySourceStorageHandle,
    source_id: &str,
    parent: Option<&str>,
    show_hidden: bool,
    request: &GetFileRowsRequest,
) -> Result<(Vec<FileEntry>, u64), FileServiceError> {
    let window = checked_page_window(request.offset, request.limit)?;
    let snapshot = source.snapshot();
    let mut cursor = None;
    let mut total = 0u64;
    let mut entries = Vec::with_capacity(window.min(PAGE_SIZE));
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
        let filtered = page.records.into_iter().filter_map(|record| {
            let entry = record_to_domain(record);
            if show_hidden || (!entry.hidden && !entry.system) {
                total = total.saturating_add(1);
                Some(entry)
            } else {
                None
            }
        });
        retain_sorted_page(
            &mut entries,
            filtered,
            window,
            request.sort_key,
            request.sort_direction,
        );
        cursor = page.next_cursor;
        if cursor.is_none() {
            break;
        }
    }
    Ok((entries, total))
}

fn scan_directory_page(
    source: &ReadOnlySourceStorageHandle,
    source_id: &str,
    parent: &str,
    offset: u64,
    limit: u32,
    show_hidden: bool,
) -> Result<(Vec<FileEntry>, u64), FileServiceError> {
    let window = checked_page_window(offset, limit)?;
    let snapshot = source.snapshot();
    let mut cursor = None;
    let mut total = 0u64;
    let mut entries = Vec::with_capacity(window.min(PAGE_SIZE));
    loop {
        let page = source
            .catalog_list_directories_by_parent(
                &snapshot,
                source_id,
                Some(parent),
                cursor.as_deref(),
                ScanDirection::Forward,
                PAGE_SIZE,
            )
            .map_err(|error| FileServiceError::other(error.to_string()))?;
        let filtered = page.records.into_iter().filter_map(|record| {
            let entry = record_to_domain(record);
            if show_hidden || !entry.hidden {
                total = total.saturating_add(1);
                Some(entry)
            } else {
                None
            }
        });
        entries.extend(filtered);
        super::metadata::sorting::sort_directories_for_tree(&mut entries);
        entries.truncate(window);
        cursor = page.next_cursor;
        if cursor.is_none() {
            break;
        }
    }
    Ok((entries, total))
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
    let repository = persistence_sqlite::repositories::file_repo::FileRepo::new(conn);
    let child_counts = repository
        .count_child_directories_batch_visible(
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
                        Some(child_counts.get(&entry.id.0).copied().unwrap_or(0) > 0),
                        mark_as_partition,
                    )
                } else {
                    super::metadata::lookup::file_entry_to_tree_node_with_partition_hint(
                        entry,
                        0,
                        Some(child_counts.get(&entry.id.0).copied().unwrap_or(0) > 0),
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
    let (entries, total) =
        scan_directory_page(&source, &source_id.0, parent_id, offset, limit, show_hidden)?;
    let start = (offset as usize).min(entries.len());
    let end = start.saturating_add(limit as usize).min(entries.len());
    let visible = &entries[start..end];
    let repository = persistence_sqlite::repositories::file_repo::FileRepo::new(conn);
    let child_counts = repository
        .count_child_directories_batch_visible(
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
                    Some(child_counts.get(&entry.id.0).copied().unwrap_or(0) > 0),
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
    let (entries, total) = scan_rows(
        &source,
        &source_id.0,
        request.parent_id.as_deref(),
        request.show_hidden,
        &request,
    )?;
    let start = (request.offset as usize).min(entries.len());
    let end = start
        .saturating_add(request.limit as usize)
        .min(entries.len());
    Ok(Some(FileRowsPageDto {
        rows: entries[start..end]
            .iter()
            .map(super::metadata::lookup::file_entry_to_dto)
            .collect::<Vec<FileEntryRowDto>>(),
        total_count: total,
        offset: request.offset,
        limit: request.limit,
        truncated: request.offset.saturating_add(request.limit as u64) < total,
    }))
}
