use crate::{
    file_service::FileServiceError, source_db, storage_runtime::ReadOnlySourceStorageHandle,
};
use domain::{DataSourceId, EntryType, FileEntry, FileEntryId};
use persistence_fjall::{CatalogEntryType, FileEntryRecord, ScanDirection};
use std::collections::HashSet;
use std::path::Path;
use transport::{
    commands::GetFileRowsRequest,
    dto::{FileChildrenDto, FileEntryRowDto, FileRowsPageDto, FileTreeNodeDto},
};

const PAGE_SIZE: usize = 256;

fn open(
    case_root: &Path,
    source_id: &DataSourceId,
) -> Result<Option<ReadOnlySourceStorageHandle>, FileServiceError> {
    let root = source_db::fjall_source_storage_root(case_root, &source_id.0)
        .map_err(|error| FileServiceError::other(error.to_string()))?;
    if !root.is_dir() {
        return Ok(None);
    }
    source_db::open_registered_fjall_source_read_only(case_root, source_id)
        .map(Some)
        .map_err(|error| FileServiceError::other(error.to_string()))
}

fn record_to_domain(record: FileEntryRecord) -> FileEntry {
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
        encrypted: record.encrypted,
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

fn all(
    source: &ReadOnlySourceStorageHandle,
    source_id: &str,
    parent: Option<&str>,
) -> Result<Vec<FileEntry>, FileServiceError> {
    let snapshot = source.snapshot();
    let mut cursor = None;
    let mut result = Vec::new();
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
        result.extend(page.records.into_iter().map(record_to_domain));
        let Some(next) = page.next_cursor else {
            break;
        };
        cursor = Some(next);
    }
    Ok(result)
}

fn all_catalog(
    source: &ReadOnlySourceStorageHandle,
    source_id: &str,
) -> Result<Vec<FileEntry>, FileServiceError> {
    let snapshot = source.snapshot();
    let mut cursor = None;
    let mut result = Vec::new();
    loop {
        let page = source
            .catalog_list_all(
                &snapshot,
                source_id,
                cursor.as_deref(),
                ScanDirection::Forward,
                PAGE_SIZE,
            )
            .map_err(|error| FileServiceError::other(error.to_string()))?;
        result.extend(page.records.into_iter().map(record_to_domain));
        let Some(next) = page.next_cursor else {
            break;
        };
        cursor = Some(next);
    }
    Ok(result)
}

pub(super) fn tree(
    case_root: &Path,
    source_id: &DataSourceId,
    show_hidden: bool,
) -> Result<Option<Vec<FileTreeNodeDto>>, FileServiceError> {
    let Some(source) = open(case_root, source_id)? else {
        return Ok(None);
    };
    let entries = all_catalog(&source, &source_id.0)?;
    let ids = entries
        .iter()
        .map(|entry| entry.id.0.clone())
        .collect::<HashSet<_>>();
    let mut entries = entries
        .into_iter()
        .filter(|entry| {
            entry.entry_type == EntryType::Directory
                && (entry.parent_id.is_none()
                    || entry
                        .parent_id
                        .as_ref()
                        .is_some_and(|parent| parent == &entry.id || !ids.contains(&parent.0)))
                && (show_hidden || (!entry.hidden && !entry.system))
        })
        .collect::<Vec<_>>();
    super::metadata::sorting::sort_directories_for_tree(&mut entries);
    Ok(Some(
        entries
            .iter()
            .map(|entry| super::metadata::lookup::file_entry_to_tree_node(entry, 0, Some(true)))
            .collect(),
    ))
}

pub(super) fn children(
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
    let mut entries = all(&source, &source_id.0, Some(parent_id))?
        .into_iter()
        .filter(|entry| entry.entry_type == EntryType::Directory && (show_hidden || !entry.hidden))
        .collect::<Vec<_>>();
    super::metadata::sorting::sort_directories_for_tree(&mut entries);
    let total = entries.len() as u64;
    let start = (offset as usize).min(entries.len());
    let end = start.saturating_add(limit as usize).min(entries.len());
    Ok(Some(FileChildrenDto {
        children: entries[start..end]
            .iter()
            .map(|entry| super::metadata::lookup::file_entry_to_tree_node(entry, 1, Some(true)))
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
    let entries = all(&source, &source_id.0, request.parent_id.as_deref())?
        .into_iter()
        .filter(|entry| request.show_hidden || (!entry.hidden && !entry.system))
        .collect::<Vec<_>>();
    let mut entries = entries;
    super::metadata::sorting::sort_entries(&mut entries, request.sort_key, request.sort_direction);
    let total = entries.len() as u64;
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
