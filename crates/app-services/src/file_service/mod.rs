use crate::datasource_service::{self, ImageFilesystemKind};
use domain::{DataSourceId, EntryType, FileEntry, FileEntryId};
use evidence_core::{EvidenceReader, FileSystemReader, RawImageReader};
use image_e01::E01Reader;
use infrastructure::constants::FILE_INSERT_BATCH_SIZE;
use persistence_sqlite::{
    repositories::{
        datasource_repo::DataSourceRepo, file_repo::FileRepo, partition_repo::PartitionRepo,
    },
    DbError, DbResult,
};
use rusqlite::Connection;
use std::{
    collections::VecDeque,
    io::Read,
    path::{Component, Path, PathBuf},
};
use transport::{
    commands::GetFileRowsRequest,
    dto::{
        DataSourcePartitionDto, DataSourceSummaryDto, FileChildrenDto, FileEntryRowDto,
        FileRowsPageDto, FileTreeNodeDto, ViewerHandleDto, ViewerRangeRequestDto,
        ViewerRangeResponseDto,
    },
};
use uuid::Uuid;

const FILE_HANDLE_PREFIX: &str = "file:";
const PARTITION_PLACEHOLDER_PREFIX: &str = "__partition_placeholder__/";

/// Statistics collected during filesystem enumeration.
pub struct EnumerationStats {
    pub file_count: u64,
    pub dir_count: u64,
    pub total_size: u64,
    pub warnings: Vec<String>,
}

/// Enumerate all files and directories from a filesystem reader and insert them
/// into the database. Uses the filesystem root name as the top-level directory name.
pub fn enumerate_filesystem(
    conn: &Connection,
    data_source_id: &DataSourceId,
    fs: &dyn FileSystemReader,
) -> DbResult<EnumerationStats> {
    enumerate_filesystem_with_root_name(conn, data_source_id, fs, None, None::<&dyn Fn(u32)>)
}

pub fn enumerate_filesystem_with_root_name(
    conn: &Connection,
    data_source_id: &DataSourceId,
    fs: &dyn FileSystemReader,
    root_name_override: Option<&str>,
    progress_fn: Option<&dyn Fn(u32)>,
) -> DbResult<EnumerationStats> {
    let repo = FileRepo::new(conn);
    let root = fs.root().map_err(|e| {
        persistence_sqlite::DbError::System(format!("Failed to read filesystem root: {}", e))
    })?;

    let root_id = FileEntryId(Uuid::new_v4().to_string());
    let root_entry = FileEntry {
        id: root_id.clone(),
        parent_id: None,
        data_source_id: data_source_id.clone(),
        path: String::new(),
        name: root_name_override.unwrap_or(&root.name).to_string(),
        entry_type: EntryType::Directory,
        size: None,
        ext: None,
        deleted: false,
        created_at: root.created_at,
        modified_at: root.modified_at,
        accessed_at: root.accessed_at,
        changed_at: None,
        hash_sha256: None,
    };

    repo.insert_batch(&[root_entry])?;
    walk_and_insert_children(&repo, fs, data_source_id, root_id, progress_fn)
}

fn compute_enumeration_progress(processed: u64) -> u64 {
    if processed < 100 {
        processed
    } else if processed < 1000 {
        50 + (processed - 100) * 30 / 900
    } else {
        80 + (processed - 1000).min(5000) * 15 / 5000
    }
}

fn walk_and_insert_children(
    repo: &FileRepo<'_>,
    fs: &dyn FileSystemReader,
    data_source_id: &DataSourceId,
    root_id: FileEntryId,
    progress_fn: Option<&dyn Fn(u32)>,
) -> DbResult<EnumerationStats> {
    let mut queue: VecDeque<(FileEntryId, String)> = VecDeque::new();
    queue.push_back((root_id, String::new()));

    let mut stats = EnumerationStats {
        file_count: 0,
        dir_count: 1,
        total_size: 0,
        warnings: Vec::new(),
    };

    let batch_size = FILE_INSERT_BATCH_SIZE;
    let mut batch: Vec<FileEntry> = Vec::with_capacity(batch_size);
    let mut total_processed: u64 = 0;

    while let Some((parent_id, dir_path)) = queue.pop_front() {
        let children = match fs.list_children(&dir_path) {
            Ok(c) => c,
            Err(e) => {
                stats
                    .warnings
                    .push(format!("Cannot read '{}': {}", dir_path, e));
                continue;
            }
        };

        for child in children {
            if child.name == "." || child.name == ".." {
                continue;
            }

            let id = FileEntryId(Uuid::new_v4().to_string());
            let entry = FileEntry {
                id: id.clone(),
                parent_id: Some(parent_id.clone()),
                data_source_id: data_source_id.clone(),
                path: child.path.clone(),
                name: child.name.clone(),
                entry_type: if child.is_dir {
                    EntryType::Directory
                } else {
                    EntryType::File
                },
                size: if child.is_dir { None } else { Some(child.size) },
                ext: child
                    .name
                    .rsplit('.')
                    .next()
                    .filter(|e| *e != child.name)
                    .map(|e| e.to_string()),
                deleted: false,
                created_at: child.created_at,
                modified_at: child.modified_at,
                accessed_at: child.accessed_at,
                changed_at: None,
                hash_sha256: None,
            };

            if child.is_dir {
                stats.dir_count += 1;
                queue.push_back((id, child.path));
            } else {
                stats.file_count += 1;
                stats.total_size += child.size;
            }

            batch.push(entry);
            total_processed += 1;

            if batch.len() >= batch_size {
                repo.insert_batch(&batch)?;
                batch.clear();
            }
            if total_processed.is_multiple_of(100) {
                if let Some(ref pf) = progress_fn {
                    let pct = compute_enumeration_progress(total_processed);
                    pf(pct as u32);
                }
            }
        }
    }

    if !batch.is_empty() {
        repo.insert_batch(&batch)?;
    }

    if let Some(ref pf) = progress_fn {
        pf(100);
    }

    Ok(stats)
}

pub fn insert_partition_placeholder_root(
    conn: &Connection,
    data_source_id: &DataSourceId,
    root_name: &str,
    status: &str,
) -> DbResult<FileEntryId> {
    let repo = FileRepo::new(conn);
    let root_id = FileEntryId(Uuid::new_v4().to_string());
    let root_entry = FileEntry {
        id: root_id.clone(),
        parent_id: None,
        data_source_id: data_source_id.clone(),
        path: format!("{PARTITION_PLACEHOLDER_PREFIX}{status}"),
        name: root_name.to_string(),
        entry_type: EntryType::Directory,
        size: None,
        ext: None,
        deleted: false,
        created_at: None,
        modified_at: None,
        accessed_at: None,
        changed_at: None,
        hash_sha256: None,
    };

    repo.insert_batch(&[root_entry])?;
    Ok(root_id)
}

pub fn replace_placeholder_root_with_real(
    conn: &Connection,
    placeholder_id: &FileEntryId,
    fs: &dyn FileSystemReader,
    root_name_override: Option<&str>,
    progress_fn: Option<&dyn Fn(u32)>,
) -> DbResult<EnumerationStats> {
    let repo = FileRepo::new(conn);
    let Some(mut root_entry) = repo.find_by_id(placeholder_id)? else {
        return Err(persistence_sqlite::DbError::System(
            "Partition placeholder root not found".to_string(),
        ));
    };

    let root = fs.root().map_err(|e| {
        persistence_sqlite::DbError::System(format!("Failed to read filesystem root: {}", e))
    })?;

    root_entry.path = String::new();
    root_entry.name = root_name_override.unwrap_or(&root.name).to_string();
    root_entry.created_at = root.created_at;
    root_entry.modified_at = root.modified_at;
    root_entry.accessed_at = root.accessed_at;

    let root_path = root_entry.path.clone();
    let root_name = root_entry.name.clone();
    let root_id_value = root_entry.id.0.clone();
    let data_source_id = root_entry.data_source_id.clone();
    let root_id = root_entry.id.clone();
    conn.execute(
        "UPDATE file_entries
         SET path = ?1, name = ?2, created_at = ?3, modified_at = ?4, accessed_at = ?5
         WHERE id = ?6",
        rusqlite::params![
            root_path,
            root_name,
            root_entry.created_at.map(|dt| dt.to_rfc3339()),
            root_entry.modified_at.map(|dt| dt.to_rfc3339()),
            root_entry.accessed_at.map(|dt| dt.to_rfc3339()),
            root_id_value,
        ],
    )?;

    walk_and_insert_children(&repo, fs, &data_source_id, root_id, progress_fn)
}

pub fn file_entry_to_dto(entry: &FileEntry) -> FileEntryRowDto {
    FileEntryRowDto {
        id: entry.id.0.clone(),
        parent_id: entry.parent_id.as_ref().map(|p| p.0.clone()),
        path: entry.path.clone(),
        name: entry.name.clone(),
        entry_type: match entry.entry_type {
            EntryType::File => "file".to_string(),
            EntryType::Directory => "directory".to_string(),
        },
        size: entry.size,
        ext: entry.ext.clone(),
        deleted: entry.deleted,
        created_at: entry.created_at.map(|dt| dt.to_rfc3339()),
        modified_at: entry.modified_at.map(|dt| dt.to_rfc3339()),
        accessed_at: entry.accessed_at.map(|dt| dt.to_rfc3339()),
        changed_at: entry.changed_at.map(|dt| dt.to_rfc3339()),
        hash_sha256: entry.hash_sha256.clone(),
    }
}

pub fn get_file_tree_real(conn: &Connection) -> Result<Vec<FileTreeNodeDto>, String> {
    let repo = FileRepo::new(conn);
    let roots = repo.find_root_directories().map_err(|e| e.to_string())?;

    // Batch-check has_children for all roots in a single pass
    let child_counts = repo
        .count_child_directories_batch(&roots.iter().map(|r| &r.id).collect::<Vec<_>>())
        .unwrap_or_default();

    roots
        .iter()
        .map(|entry| {
            let has_children = child_counts.get(&entry.id.0).copied().unwrap_or(0) > 0;
            Ok(file_entry_to_tree_node(entry, 0, Some(has_children)))
        })
        .collect()
}

pub fn get_file_children_lazy(
    conn: &Connection,
    parent_id: &str,
    offset: u64,
    limit: u32,
) -> Result<FileChildrenDto, String> {
    let repo = FileRepo::new(conn);
    let parent = match repo
        .find_by_id(&FileEntryId(parent_id.to_string()))
        .map_err(|e| e.to_string())?
    {
        Some(entry) if entry.entry_type == EntryType::Directory => entry,
        _ => {
            return Ok(FileChildrenDto {
                children: vec![],
                total_count: 0,
                offset: Some(offset),
                limit: Some(limit),
                truncated: Some(false),
            })
        }
    };

    let children = repo
        .find_child_directories_page(&parent.id, offset, limit)
        .map_err(|e| e.to_string())?;
    let total_count = repo
        .count_child_directories(&parent.id)
        .map_err(|e| e.to_string())?;
    let child_depth = directory_depth(&parent).saturating_add(1);

    // Batch-check has_children for all children
    let child_counts = repo
        .count_child_directories_batch(&children.iter().map(|c| &c.id).collect::<Vec<_>>())
        .unwrap_or_default();

    let child_nodes = children
        .iter()
        .map(|entry| {
            let has_children = child_counts.get(&entry.id.0).copied().unwrap_or(0) > 0;
            file_entry_to_tree_node(entry, child_depth, Some(has_children))
        })
        .collect();

    Ok(FileChildrenDto {
        children: child_nodes,
        total_count,
        offset: Some(offset),
        limit: Some(limit),
        truncated: Some(offset + (limit as u64) < total_count),
    })
}

pub fn get_file_rows_for_request(
    conn: &Connection,
    request: &GetFileRowsRequest,
) -> Result<FileRowsPageDto, String> {
    let mut request = request.clone();
    request.validate()?;
    let repo = FileRepo::new(conn);
    let (entries, total_count) = match request.parent_id.as_deref() {
        Some(parent_id) => {
            let parent = repo
                .find_by_id(&FileEntryId(parent_id.to_string()))
                .map_err(|e| e.to_string())?;
            match parent {
                Some(entry) if entry.entry_type == EntryType::Directory => {
                    let entries = repo
                        .find_children_page(&entry.id, request.offset, request.limit)
                        .map_err(|e| e.to_string())?;
                    let total_count = repo.count_children(&entry.id).map_err(|e| e.to_string())?;
                    (entries, total_count)
                }
                _ => (Vec::new(), 0),
            }
        }
        None => {
            let entries = repo
                .find_root_entries_page(request.offset, request.limit)
                .map_err(|e| e.to_string())?;
            let total_count = repo.count_root_entries().map_err(|e| e.to_string())?;
            (entries, total_count)
        }
    };

    Ok(FileRowsPageDto {
        rows: entries.iter().map(file_entry_to_dto).collect(),
        total_count,
        offset: request.offset,
        limit: request.limit,
        truncated: request.offset + (request.limit as u64) < total_count,
    })
}

pub fn get_data_sources_real(
    conn: &Connection,
    case_id: &domain::CaseId,
) -> Result<Vec<DataSourceSummaryDto>, String> {
    let ds_repo = DataSourceRepo::new(conn);
    let file_repo = FileRepo::new(conn);
    let partition_repo = PartitionRepo::new(conn);
    let sources = ds_repo.find_by_case(case_id).map_err(|e| e.to_string())?;

    Ok(sources
        .into_iter()
        .map(|source| {
            let partitions = partition_repo
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

            DataSourceSummaryDto {
                id: source.id.0.clone(),
                name: source.name,
                kind: source.kind.to_string(),
                source_path: source.source_path.display().to_string(),
                imported_at: source.imported_at.to_rfc3339(),
                file_count: file_repo.count_by_data_source(&source.id).ok(),
                partitions,
            }
        })
        .collect())
}

pub fn rename_data_source_real(
    conn: &Connection,
    data_source_id: &str,
    name: &str,
) -> Result<(), String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err("Data source name cannot be empty".to_string());
    }

    DataSourceRepo::new(conn)
        .rename(&DataSourceId(data_source_id.to_string()), trimmed)
        .map_err(|e| e.to_string())
}

pub fn get_recent_objects_real(
    conn: &Connection,
) -> Result<Vec<transport::dto::RecentObjectDto>, String> {
    let file_repo = FileRepo::new(conn);
    let roots = file_repo.find_root_entries().map_err(|e| e.to_string())?;
    let mut recent = Vec::new();
    let mut queue: VecDeque<FileEntry> = roots.into();

    while let Some(entry) = queue.pop_front() {
        if entry.entry_type == EntryType::Directory {
            let children = file_repo
                .find_children(&entry.id)
                .map_err(|e| e.to_string())?;
            queue.extend(children);
            continue;
        }

        let timestamp = entry
            .modified_at
            .or(entry.created_at)
            .or(entry.accessed_at)
            .map(|dt| dt.to_rfc3339())
            .unwrap_or_else(|| "-".to_string());

        recent.push(transport::dto::RecentObjectDto {
            id: entry.id.0.clone(),
            title: entry.name.clone(),
            detail: if entry.path.is_empty() {
                entry.name.clone()
            } else {
                entry.path.clone()
            },
            time: timestamp,
            kind: "file".to_string(),
        });

        if recent.len() >= 8 {
            break;
        }
    }

    Ok(recent)
}

pub fn open_file_handle_real(conn: &Connection, file_id: &str) -> Result<ViewerHandleDto, String> {
    let repo = FileRepo::new(conn);
    let entry = repo
        .find_by_id(&FileEntryId(file_id.to_string()))
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "File not found".to_string())?;

    if entry.entry_type != EntryType::File {
        return Err("Cannot open a directory as a file".to_string());
    }

    Ok(ViewerHandleDto {
        handle_id: format!("{FILE_HANDLE_PREFIX}{}", entry.id.0),
        size: entry.size.unwrap_or(0),
        mime: mime_for_entry(&entry),
    })
}

pub fn read_file_range_for_case(
    conn: &Connection,
    request: &ViewerRangeRequestDto,
) -> Result<ViewerRangeResponseDto, String> {
    let mut request = request.clone();
    request.validate()?;
    let file_id = file_id_from_handle(&request.handle_id)?;
    let mut file = open_file_content_by_id(conn, &FileEntryId(file_id.to_string()))?;

    skip_reader_bytes(file.as_mut(), request.offset)?;
    let length = (request.length as usize).min(infrastructure::constants::MAX_RANGE_LENGTH);
    let mut bytes = vec![0u8; length];
    let read = file.read(&mut bytes).map_err(|e| e.to_string())?;
    bytes.truncate(read);

    Ok(ViewerRangeResponseDto {
        kind: "hex".into(),
        lines: format_hex_lines(request.offset, &bytes),
        encoding: None,
    })
}

pub fn read_file_range_real(_request: &ViewerRangeRequestDto) -> ViewerRangeResponseDto {
    empty_hex_response()
}

/// Open real evidence content for a file entry.
///
/// The caller provides a `FileEntryId`; this helper resolves the backing data
/// source and applies the same logical-directory and image-backed safety checks
/// used by preview reads.
pub fn open_file_content_by_id(
    conn: &Connection,
    file_id: &FileEntryId,
) -> Result<Box<dyn Read>, String> {
    let repo = FileRepo::new(conn);
    let entry = repo
        .find_by_id(file_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "File not found".to_string())?;

    open_file_content_for_entry(&repo, &entry)
}

pub fn read_file_header_by_id(
    conn: &Connection,
    file_id: &FileEntryId,
    max_bytes: usize,
) -> Result<Vec<u8>, String> {
    let mut reader = open_file_content_by_id(conn, file_id)?;
    let mut limited = reader.by_ref().take(max_bytes as u64);
    let mut bytes = Vec::new();
    limited.read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    Ok(bytes)
}

fn open_file_content_for_entry(
    repo: &FileRepo<'_>,
    entry: &FileEntry,
) -> Result<Box<dyn Read>, String> {
    if entry.entry_type != EntryType::File {
        return Err("Cannot read a directory as a file".to_string());
    }

    let (kind, source_path) = repo
        .find_data_source_location(&entry.data_source_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Data source not found".to_string())?;
    let expected_partition_index = root_partition_index_for_entry(repo, entry);

    match kind.as_str() {
        "logical_directory" => open_logical_file(&source_path, entry),
        "e01" => open_e01_file(&source_path, entry, expected_partition_index),
        "raw" => open_raw_file(&source_path, entry, expected_partition_index),
        other => Err(format!(
            "Range reading is not yet wired for data source kind '{}'",
            other
        )),
    }
}

fn file_entry_to_tree_node(
    entry: &FileEntry,
    depth: u32,
    expanded: Option<bool>,
) -> FileTreeNodeDto {
    let (node_type, status) = if let Some(partition_status) = partition_placeholder_status(entry) {
        ("partition".to_string(), Some(partition_status.to_string()))
    } else if depth == 0 && looks_like_partition_root_name(&entry.name) {
        ("partition".to_string(), Some("ready".to_string()))
    } else {
        ("directory".to_string(), None)
    };

    FileTreeNodeDto {
        id: entry.id.0.clone(),
        name: entry.name.clone(),
        depth,
        has_children: entry.entry_type == EntryType::Directory,
        entry_type: Some(match entry.entry_type {
            EntryType::Directory => "directory".to_string(),
            EntryType::File => "file".to_string(),
        }),
        size: entry.size,
        node_type: Some(node_type),
        status,
        expanded,
        active: Some(false),
    }
}

fn directory_depth(entry: &FileEntry) -> u32 {
    if entry.path.is_empty() {
        return 0;
    }

    Path::new(&entry.path)
        .components()
        .filter(|component| matches!(component, Component::Normal(_)))
        .count() as u32
}

fn partition_placeholder_status(entry: &FileEntry) -> Option<&str> {
    entry
        .path
        .strip_prefix(PARTITION_PLACEHOLDER_PREFIX)
        .filter(|status| !status.is_empty())
}

fn looks_like_partition_root_name(name: &str) -> bool {
    name.starts_with("Partition ") || name.starts_with("Volume")
}

fn mime_for_entry(entry: &FileEntry) -> Option<String> {
    let ext = entry.ext.as_deref()?.to_ascii_lowercase();
    let mime = match ext.as_str() {
        "txt" | "log" | "csv" | "md" => "text/plain",
        "json" => "application/json",
        "html" | "htm" => "text/html",
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "pdf" => "application/pdf",
        _ => "application/octet-stream",
    };
    Some(mime.to_string())
}

pub fn store_data_source_partitions(
    conn: &Connection,
    data_source_id: &DataSourceId,
    partitions: &[crate::datasource_service::PartitionRecord],
) -> Result<(), String> {
    let repo = PartitionRepo::new(conn);
    let records = partitions
        .iter()
        .map(|partition| {
            persistence_sqlite::repositories::partition_repo::DataSourcePartitionRecord {
                id: Uuid::new_v4().to_string(),
                data_source_id: data_source_id.0.clone(),
                partition_index: partition.index as u32,
                name: partition.name.clone(),
                kind_label: partition.kind_label.clone(),
                status: partition_status_label(partition.status).to_string(),
                type_guid: partition.type_guid.clone(),
                offset: partition.offset,
                length: partition.length,
                filesystem: partition.filesystem.map(image_filesystem_kind_label),
                unlock_hint: partition_unlock_hint(partition),
            }
        })
        .collect::<Vec<_>>();

    repo.replace_for_data_source(&data_source_id.0, &records)
        .map_err(|e| e.to_string())
}

fn partition_status_label(status: crate::datasource_service::PartitionStatus) -> &'static str {
    match status {
        crate::datasource_service::PartitionStatus::Supported => "supported",
        crate::datasource_service::PartitionStatus::EncryptedBitLocker => "locked",
        crate::datasource_service::PartitionStatus::Unsupported => "unsupported",
    }
}

fn image_filesystem_kind_label(kind: crate::datasource_service::ImageFilesystemKind) -> String {
    match kind {
        crate::datasource_service::ImageFilesystemKind::Ntfs => "NTFS".to_string(),
        crate::datasource_service::ImageFilesystemKind::Fat => "FAT".to_string(),
        crate::datasource_service::ImageFilesystemKind::BitLocker => "BitLocker".to_string(),
    }
}

fn partition_unlock_hint(partition: &crate::datasource_service::PartitionRecord) -> Option<String> {
    if partition.status == crate::datasource_service::PartitionStatus::EncryptedBitLocker {
        Some("BitLocker 分区需要先解锁后才能浏览文件内容。".to_string())
    } else {
        None
    }
}

fn file_id_from_handle(handle_id: &str) -> Result<&str, String> {
    handle_id
        .strip_prefix(FILE_HANDLE_PREFIX)
        .filter(|file_id| !file_id.is_empty())
        .ok_or_else(|| "Invalid file handle".to_string())
}

/// Get the full file path for a file entry.
pub fn get_file_path_for_entry(conn: &Connection, file_id: &str) -> Result<PathBuf, String> {
    let repo = FileRepo::new(conn);
    let entry = repo
        .find_by_id(&FileEntryId(file_id.to_string()))
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "File not found".to_string())?;

    let (kind, source_path) = repo
        .find_data_source_location(&entry.data_source_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Data source not found".to_string())?;

    if kind == "logical_directory" {
        let root = PathBuf::from(&source_path)
            .canonicalize()
            .map_err(|e| format!("Cannot access data source root: {}", e))?;
        let relative_path = safe_relative_path(&entry.path)?;
        Ok(root.join(relative_path))
    } else {
        Err("File path only available for logical directories".to_string())
    }
}

fn open_logical_file(source_path: &str, entry: &FileEntry) -> Result<Box<dyn Read>, String> {
    let root = PathBuf::from(source_path)
        .canonicalize()
        .map_err(|e| format!("Cannot access data source root: {}", e))?;
    let relative_path = safe_relative_path(&entry.path)?;
    let full_path = root.join(relative_path);

    // Check for symlinks in the path before canonicalizing
    let mut check_path = PathBuf::new();
    for component in full_path.components() {
        check_path.push(component);
        if check_path.is_symlink() {
            return Err(format!(
                "Symlink detected in path at '{}' — rejected for security",
                check_path.display()
            ));
        }
    }

    let canonical = full_path
        .canonicalize()
        .map_err(|e| format!("Cannot access file '{}': {}", entry.path, e))?;

    if !canonical.starts_with(&root) {
        return Err("File path escapes data source root".to_string());
    }

    if !canonical.is_file() {
        return Err("File entry does not point to a regular file".to_string());
    }

    std::fs::File::open(canonical)
        .map(|file| Box::new(file) as Box<dyn Read>)
        .map_err(|e| e.to_string())
}

fn open_raw_file(
    source_path: &str,
    entry: &FileEntry,
    expected_partition_index: Option<usize>,
) -> Result<Box<dyn Read>, String> {
    let reader = RawImageReader::open(Path::new(source_path)).map_err(|e| e.to_string())?;
    open_image_file(entry, reader, expected_partition_index)
}

fn open_e01_file(
    source_path: &str,
    entry: &FileEntry,
    expected_partition_index: Option<usize>,
) -> Result<Box<dyn Read>, String> {
    let reader = E01Reader::open(Path::new(source_path)).map_err(|e| e.to_string())?;
    open_image_file(entry, reader, expected_partition_index)
}

fn open_image_file<R>(
    entry: &FileEntry,
    mut reader: R,
    expected_partition_index: Option<usize>,
) -> Result<Box<dyn Read>, String>
where
    R: EvidenceReader + Read + std::io::Seek + 'static,
{
    let probe =
        datasource_service::detect_image_filesystem(&mut reader).map_err(|e| e.to_string())?;
    if probe.candidates.is_empty() {
        let detail = if probe.warnings.is_empty() {
            "No supported NTFS/FAT filesystem detected".to_string()
        } else {
            probe.warnings.join("; ")
        };
        return Err(detail);
    }

    let source_path = reader.info().path.clone();
    let source_kind = reader.info().kind.clone();
    for candidate in probe.candidates {
        if let Some(expected_partition) = expected_partition_index {
            if candidate.partition_index != Some(expected_partition) {
                continue;
            }
        }

        let boxed_reader: Box<dyn EvidenceReader> = match source_kind.as_str() {
            "e01" => Box::new(E01Reader::open(&source_path).map_err(|e| e.to_string())?),
            _ => Box::new(RawImageReader::open(&source_path).map_err(|e| e.to_string())?),
        };

        let result = match candidate.kind {
            ImageFilesystemKind::Ntfs => {
                let fs = fs_ntfs::NtfsReader::open(boxed_reader, candidate.offset)
                    .map_err(|e| e.to_string())?;
                fs.open_file(&entry.path)
                    .map_err(|e| format!("Cannot open NTFS file '{}': {}", entry.path, e))
            }
            ImageFilesystemKind::Fat => {
                let fs = fs_fat::FatReader::open(boxed_reader, candidate.offset)
                    .map_err(|e| e.to_string())?;
                fs.open_file(&entry.path)
                    .map_err(|e| format!("Cannot open FAT file '{}': {}", entry.path, e))
            }
            ImageFilesystemKind::BitLocker => Err(format!(
                "Cannot open '{}' from locked BitLocker partition",
                entry.path
            )),
        };

        if result.is_ok() {
            return result;
        }
    }

    Err(format!(
        "Cannot open image-backed file '{}' from any detected partition",
        entry.path
    ))
}

fn root_partition_index_for_entry(repo: &FileRepo<'_>, entry: &FileEntry) -> Option<usize> {
    if let Some(index) = mft_partition_index_from_entry_id(&entry.id.0) {
        return Some(index);
    }

    let mut current = entry.clone();
    while let Some(parent_id) = &current.parent_id {
        let parent = repo.find_by_id(parent_id).ok()??;
        current = parent;
    }

    current
        .name
        .strip_prefix("Partition ")?
        .chars()
        .take_while(|ch| ch.is_ascii_digit())
        .collect::<String>()
        .parse()
        .ok()
}

fn mft_partition_index_from_entry_id(entry_id: &str) -> Option<usize> {
    let mut parts = entry_id.split(':');
    match (parts.next(), parts.next(), parts.next(), parts.next()) {
        (Some("mft"), Some(partition), Some(_record), None) => partition.parse().ok(),
        _ => None,
    }
}

pub fn safe_relative_path(path: &str) -> Result<PathBuf, String> {
    // Reject empty paths
    if path.is_empty() {
        return Err("Empty file path".to_string());
    }

    // Reject URL-encoded traversal attempts
    let decoded = urlencoding_decode(path);
    if decoded != path {
        // Check if decoded version contains traversal
        if decoded.contains("..") || decoded.contains('/') || decoded.contains('\\') {
            return Err("URL-encoded traversal detected".to_string());
        }
    }

    let mut safe = PathBuf::new();
    for component in Path::new(path).components() {
        match component {
            Component::Normal(part) => {
                let s = part.to_str().ok_or("Invalid UTF-8 in path")?;
                // Reject null byte injection
                if s.contains('\0') {
                    return Err("Null byte in path".to_string());
                }
                // Reject Windows reserved names
                if is_windows_reserved_name(s) {
                    return Err(format!("Reserved name: {}", s));
                }
                safe.push(part);
            }
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err("Unsafe file path in catalog".to_string());
            }
        }
    }

    // Reject excessively long paths
    if safe.as_os_str().len() > infrastructure::constants::MAX_PATH_LENGTH {
        return Err("Path too long".to_string());
    }

    Ok(safe)
}

/// Decode URL-encoded characters in a path
fn urlencoding_decode(path: &str) -> String {
    let mut result = String::with_capacity(path.len());
    let mut chars = path.chars();
    while let Some(c) = chars.next() {
        if c == '%' {
            let hex: String = chars.by_ref().take(2).collect();
            if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                result.push(byte as char);
            } else {
                result.push(c);
                result.push_str(&hex);
            }
        } else {
            result.push(c);
        }
    }
    result
}

/// Check if a filename is a Windows reserved name
fn is_windows_reserved_name(name: &str) -> bool {
    const RESERVED: &[&str] = &[
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    let upper = name.to_ascii_uppercase();
    let stem = upper.split('.').next().unwrap_or(&upper);
    RESERVED.contains(&stem)
}

pub fn skip_reader_bytes(reader: &mut dyn Read, mut remaining: u64) -> Result<(), String> {
    // 使用更大的缓冲区减少系统调用次数
    let mut buffer = vec![0u8; 65536]; // 64KB buffer
    while remaining > 0 {
        let chunk_len = remaining.min(buffer.len() as u64) as usize;
        let read = reader
            .read(&mut buffer[..chunk_len])
            .map_err(|e| e.to_string())?;
        if read == 0 {
            return Err("Read offset exceeds file size".to_string());
        }
        remaining -= read as u64;
    }
    Ok(())
}

fn format_hex_lines(base_offset: u64, bytes: &[u8]) -> Vec<String> {
    // 预计算容量：每行最多 8(offset) + 2(spaces) + 16*3(hex) + 16(space) = 74 字符
    let line_count = bytes.len().div_ceil(16);
    let mut result = Vec::with_capacity(line_count);

    for (line_idx, chunk) in bytes.chunks(16).enumerate() {
        let offset = base_offset + (line_idx * 16) as u64;
        // 预分配：8(offset) + 2(spaces) + chunk.len()*3(hex) + chunk.len()-1(spaces)
        let mut line = String::with_capacity(8 + 2 + chunk.len() * 4);

        // 写入偏移量
        use std::fmt::Write;
        let _ = write!(line, "{offset:08X}");
        line.push_str("  ");

        // 写入 hex 字节
        for (i, byte) in chunk.iter().enumerate() {
            if i > 0 {
                line.push(' ');
            }
            let _ = write!(line, "{byte:02X}");
        }

        result.push(line);
    }
    result
}

fn empty_hex_response() -> ViewerRangeResponseDto {
    ViewerRangeResponseDto {
        kind: "hex".into(),
        lines: Vec::new(),
        encoding: None,
    }
}

// ============================================================================
// MFT-based bulk NTFS enumeration with multi-threading
// ============================================================================

use crossbeam_channel::{bounded, Receiver, Sender};
use fs_ntfs::mft_scanner::{MftRecord, MftScanner};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;

const MFT_CHUNK_RECORDS: u64 = 10_000;
const MFT_CHANNEL_BOUND: usize = 4;
const MFT_DB_BATCH_SIZE: usize = 2_000;

/// Multi-threaded MFT-based NTFS enumeration.
///
/// Architecture:
///   Reader Thread → channel → Parser Thread Pool → channel → DB Writer Thread
///
/// - Reader: Sequentially reads MFT chunks from E01
/// - Parsers: Parse FILE records in parallel (CPU-bound)
/// - Writer: Batch-inserts FileEntry into SQLite
///
/// After all records are processed, reconstructs full paths via parent_ref chains.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn enumerate_filesystem_mft(
    conn: &Connection,
    data_source_id: &DataSourceId,
    e01_path: &Path,
    volume_offset: u64,
    mft_cluster: u64,
    cluster_size: u64,
    record_size: u32,
    bytes_per_sector: u16,
    mft_data_size: u64,
    progress_fn: Option<&dyn Fn(u32, &str)>,
    cancel: Option<Arc<AtomicBool>>,
) -> DbResult<EnumerationStats> {
    let scanner = MftScanner::new(
        volume_offset,
        mft_cluster,
        cluster_size,
        record_size,
        bytes_per_sector,
        mft_data_size,
    );
    let total_records = scanner.total_records();
    let scanner_record_size = scanner.record_size();
    let mft_abs_offset = scanner.mft_abs_offset();

    if let Some(pf) = progress_fn {
        pf(5, "Starting MFT scan...");
    }

    // --- Channel setup ---
    // reader → parser: raw MFT chunk buffers
    let (chunk_tx, chunk_rx): (Sender<MftChunk>, Receiver<MftChunk>) = bounded(MFT_CHANNEL_BOUND);
    // parser → writer: parsed FileEntry batches
    let (entry_tx, entry_rx): (Sender<Vec<FileEntry>>, Receiver<Vec<FileEntry>>) =
        bounded(MFT_CHANNEL_BOUND);

    let processed = Arc::new(AtomicU64::new(0));
    let errors = Arc::new(AtomicU64::new(0));

    // --- Reader thread ---
    let reader_path = e01_path.to_path_buf();
    let reader_processed = processed.clone();
    let reader_cancel = cancel.clone();

    let reader_handle = thread::Builder::new()
        .name("mft-reader".into())
        .spawn(move || {
            // Each reader thread opens its own E01Reader
            let mut reader = match E01Reader::open(&reader_path) {
                Ok(r) => r,
                Err(e) => {
                    tracing::error!("MFT reader: failed to open E01: {}", e);
                    return;
                }
            };

            let mut start_record = 0u64;
            while start_record < total_records {
                // Check cancel
                if let Some(ref cancel) = reader_cancel {
                    if cancel.load(Ordering::Relaxed) {
                        tracing::info!("MFT reader: cancelled");
                        return;
                    }
                }

                let chunk_count = MFT_CHUNK_RECORDS.min(total_records - start_record);
                let byte_offset = mft_abs_offset + start_record * scanner_record_size as u64;
                let byte_count = chunk_count * scanner_record_size as u64;

                // Read chunk from E01
                use std::io::{Read, Seek, SeekFrom};
                if reader.seek(SeekFrom::Start(byte_offset)).is_err() {
                    break;
                }
                let mut buf = vec![0u8; byte_count as usize];
                if reader.read_exact(&mut buf).is_err() {
                    tracing::warn!("MFT reader: read error at record {}", start_record);
                    break;
                }

                let chunk = MftChunk {
                    data: buf,
                    start_record,
                    count: chunk_count,
                };

                if chunk_tx.send(chunk).is_err() {
                    break; // channel closed
                }

                start_record += chunk_count;
                reader_processed.store(start_record, Ordering::Relaxed);
            }
            // Drop chunk_tx to signal EOF to parsers
            drop(chunk_tx);
        })
        .map_err(|e| DbError::System(format!("Failed to spawn MFT reader: {}", e)))?;

    // --- Parser thread pool ---
    let num_parsers = num_cpus::get().clamp(2, 8);
    let mut parser_handles = Vec::with_capacity(num_parsers);

    for parser_id in 0..num_parsers {
        let rx = chunk_rx.clone();
        let tx = entry_tx.clone();
        let ds_id = data_source_id.clone();

        let handle = thread::Builder::new()
            .name(format!("mft-parser-{}", parser_id))
            .spawn(move || {
                let scanner = MftScanner::new(
                    volume_offset,
                    mft_cluster,
                    cluster_size,
                    scanner_record_size,
                    bytes_per_sector,
                    mft_data_size,
                );

                for chunk in rx.iter() {
                    let records = scanner.parse_chunk(&chunk.data, chunk.start_record, chunk.count);
                    let entries = records_to_file_entries(&records, &ds_id);
                    if !entries.is_empty() && tx.send(entries).is_err() {
                        break;
                    }
                }
            })
            .map_err(|e| DbError::System(format!("Failed to spawn MFT parser: {}", e)))?;

        parser_handles.push(handle);
    }

    // Drop our copy of chunk_rx and entry_tx so channels close properly
    drop(chunk_rx);
    drop(entry_tx);

    // --- Writer thread (runs on current thread for SQLite safety) ---
    let repo = FileRepo::new(conn);
    let mut total_files = 0u64;
    let mut total_dirs = 0u64;
    let mut total_size = 0u64;
    let mut warnings = Vec::new();
    let mut batch: Vec<FileEntry> = Vec::with_capacity(MFT_DB_BATCH_SIZE);
    let mut path_map: HashMap<String, (Option<String>, String, bool)> = HashMap::new();

    for entry_batch in entry_rx.iter() {
        for mut entry in entry_batch {
            match entry.entry_type {
                EntryType::File => {
                    total_files += 1;
                    total_size += entry.size.unwrap_or(0);
                }
                EntryType::Directory => total_dirs += 1,
            }

            add_entry_to_path_map(&mut path_map, &entry);

            // MFT parents can appear later than their children. Insert with a
            // temporary null parent, then restore parent links after all rows
            // exist so SQLite self-referential foreign keys are satisfied.
            entry.parent_id = None;
            batch.push(entry);

            if batch.len() >= MFT_DB_BATCH_SIZE {
                if let Err(e) = repo.insert_batch(&batch) {
                    warnings.push(format!("DB insert error: {}", e));
                    errs_add(&errors);
                }
                batch.clear();
            }
        }

        // Progress
        let done = processed.load(Ordering::Relaxed);
        if let Some(pf) = progress_fn {
            let pct = ((done as f64 / total_records as f64) * 90.0) as u32;
            pf(
                5 + pct,
                &format!("Scanned {} / {} MFT records", done, total_records),
            );
        }
    }

    // Flush remaining batch
    if !batch.is_empty() {
        if let Err(e) = repo.insert_batch(&batch) {
            warnings.push(format!("DB insert error: {}", e));
        }
    }

    // Wait for reader and parsers
    if let Err(e) = reader_handle.join() {
        warnings.push(format!("MFT reader thread panicked: {:?}", e));
        tracing::error!("MFT reader thread panicked: {:?}", e);
    }
    for h in parser_handles {
        if let Err(e) = h.join() {
            warnings.push(format!("MFT parser thread panicked: {:?}", e));
            tracing::error!("MFT parser thread panicked: {:?}", e);
        }
    }

    if let Some(pf) = progress_fn {
        pf(95, "Reconstructing paths...");
    }

    update_entry_paths(conn, data_source_id, &path_map)?;
    update_entry_parent_ids(conn, data_source_id, &path_map)?;

    if let Some(pf) = progress_fn {
        pf(100, "MFT scan complete");
    }

    Ok(EnumerationStats {
        file_count: total_files,
        dir_count: total_dirs,
        total_size,
        warnings,
    })
}

/// Internal chunk of raw MFT data.
struct MftChunk {
    data: Vec<u8>,
    start_record: u64,
    count: u64,
}

/// Convert parsed MFT records to domain FileEntry objects.
fn records_to_file_entries(records: &[MftRecord], data_source_id: &DataSourceId) -> Vec<FileEntry> {
    records
        .iter()
        .filter(|r| r.is_valid && (!r.name.is_empty() || r.record_number == 5))
        .map(|r| {
            let name = if r.record_number == 5 && (r.name.is_empty() || r.name == ".") {
                "\\".to_string()
            } else {
                r.name.clone()
            };
            let entry_type = if r.is_dir {
                EntryType::Directory
            } else {
                EntryType::File
            };
            let ext = if r.is_dir {
                None
            } else {
                r.name
                    .rsplit('.')
                    .next()
                    .filter(|e| *e != r.name)
                    .map(|e| e.to_string())
            };
            FileEntry {
                id: FileEntryId(format!("mft:{}", r.record_number)),
                parent_id: if r.record_number == 5 {
                    None
                } else {
                    Some(FileEntryId(format!("mft:{}", r.parent_ref)))
                },
                data_source_id: data_source_id.clone(),
                path: String::new(), // filled in during path reconstruction
                name,
                entry_type,
                size: if r.is_dir { None } else { Some(r.size) },
                ext,
                deleted: false,
                created_at: r.created_at,
                modified_at: r.modified_at,
                accessed_at: r.accessed_at,
                changed_at: r.changed_at,
                hash_sha256: None,
            }
        })
        .collect()
}

fn add_entry_to_path_map(
    path_map: &mut HashMap<String, (Option<String>, String, bool)>,
    entry: &FileEntry,
) {
    let record_num = entry.id.0.strip_prefix("mft:").unwrap_or(&entry.id.0);
    let parent_num = entry
        .parent_id
        .as_ref()
        .and_then(|p| p.0.strip_prefix("mft:").map(|s| s.to_string()));
    path_map.insert(
        record_num.to_string(),
        (
            parent_num,
            entry.name.clone(),
            entry.entry_type == EntryType::Directory,
        ),
    );
}

/// Reconstruct full paths from parent_ref chains and update DB entries.
///
/// Uses recursive resolution with caching for O(n) complexity.
fn update_entry_paths(
    conn: &Connection,
    data_source_id: &DataSourceId,
    path_map: &HashMap<String, (Option<String>, String, bool)>,
) -> DbResult<()> {
    let mut resolved: HashMap<String, String> = HashMap::with_capacity(path_map.len());
    let mut visiting: HashSet<String> = HashSet::new(); // Cycle detection

    // Recursive path resolution with caching
    fn resolve_path(
        record: &str,
        path_map: &HashMap<String, (Option<String>, String, bool)>,
        resolved: &mut HashMap<String, String>,
        visiting: &mut HashSet<String>,
    ) -> String {
        // Already resolved
        if let Some(path) = resolved.get(record) {
            return path.clone();
        }

        // Cycle detection
        if !visiting.insert(record.to_string()) {
            tracing::warn!("Cycle detected in path chain at record {}", record);
            return String::new();
        }

        let (parent, name, _) = match path_map.get(record) {
            Some(entry) => entry,
            None => {
                visiting.remove(record);
                return String::new();
            }
        };

        let path = match parent {
            Some(p) if p != "5" && path_map.contains_key(p) => {
                let parent_path = resolve_path(p, path_map, resolved, visiting);
                if parent_path.is_empty() {
                    name.clone()
                } else {
                    format!("{}/{}", parent_path, name)
                }
            }
            _ => name.clone(), // Root entry
        };

        resolved.insert(record.to_string(), path.clone());
        visiting.remove(record);
        path
    }

    // Resolve all entries
    let records: Vec<String> = path_map.keys().cloned().collect();
    for record in &records {
        resolve_path(record, path_map, &mut resolved, &mut visiting);
    }

    // Update DB in batches
    let tx = conn.unchecked_transaction()?;
    {
        let mut stmt =
            tx.prepare("UPDATE file_entries SET path = ?1 WHERE id = ?2 AND data_source_id = ?3")?;
        for (record_num, path) in &resolved {
            let entry_id = format!("mft:{}", record_num);
            stmt.execute(rusqlite::params![path, entry_id, data_source_id.0])?;
        }
    }
    tx.commit()?;
    Ok(())
}

fn update_entry_parent_ids(
    conn: &Connection,
    data_source_id: &DataSourceId,
    path_map: &HashMap<String, (Option<String>, String, bool)>,
) -> DbResult<()> {
    let tx = conn.unchecked_transaction()?;
    {
        let mut stmt = tx.prepare(
            "UPDATE file_entries SET parent_id = ?1 WHERE id = ?2 AND data_source_id = ?3",
        )?;
        for (record_num, (parent, _, _)) in path_map {
            let entry_id = format!("mft:{}", record_num);
            let parent_id = mft_parent_entry_id(record_num, parent.as_deref(), path_map);
            stmt.execute(rusqlite::params![parent_id, entry_id, data_source_id.0])?;
        }
    }
    tx.commit()?;
    Ok(())
}

fn mft_parent_entry_id(
    record_num: &str,
    parent_num: Option<&str>,
    path_map: &HashMap<String, (Option<String>, String, bool)>,
) -> Option<String> {
    if record_num == "5" {
        return None;
    }

    match parent_num {
        Some(parent) if parent != record_num && path_map.contains_key(parent) => {
            Some(format!("mft:{}", parent))
        }
        _ if path_map.contains_key("5") => Some("mft:5".to_string()),
        _ => None,
    }
}

fn errs_add(errors: &Arc<AtomicU64>) {
    errors.fetch_add(1, Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_path_rejects_dot_dot_traversal() {
        assert!(safe_relative_path("../etc/passwd").is_err());
        assert!(safe_relative_path("foo/../../bar").is_err());
        assert!(safe_relative_path("..\\windows\\system32").is_err());
    }

    #[test]
    fn safe_path_rejects_url_encoded_traversal() {
        assert!(safe_relative_path("%2e%2e%2fetc%2fpasswd").is_err());
        assert!(safe_relative_path("foo%2f%2e%2e%2fbar").is_err());
    }

    #[test]
    fn safe_path_rejects_null_byte() {
        assert!(safe_relative_path("file.txt\0.jpg").is_err());
    }

    #[test]
    fn safe_path_rejects_absolute_path() {
        assert!(safe_relative_path("/etc/passwd").is_err());
    }

    #[test]
    fn safe_path_accepts_valid_paths() {
        assert!(safe_relative_path("documents/file.txt").is_ok());
        assert!(safe_relative_path("a/b/c.txt").is_ok());
        assert!(safe_relative_path("simple.txt").is_ok());
    }

    #[test]
    fn safe_path_rejects_empty_path() {
        assert!(safe_relative_path("").is_err());
    }

    #[test]
    fn safe_path_rejects_windows_reserved_names() {
        assert!(safe_relative_path("CON").is_err());
        assert!(safe_relative_path("NUL.txt").is_err());
        assert!(safe_relative_path("COM1").is_err());
        assert!(safe_relative_path("LPT1.dat").is_err());
    }

    #[test]
    fn mft_root_record_becomes_tree_root() {
        let records = vec![
            MftRecord {
                record_number: 5,
                name: ".".to_string(),
                parent_ref: 5,
                is_dir: true,
                size: 0,
                created_at: None,
                modified_at: None,
                accessed_at: None,
                changed_at: None,
                is_valid: true,
            },
            MftRecord {
                record_number: 42,
                name: "Windows".to_string(),
                parent_ref: 5,
                is_dir: true,
                size: 0,
                created_at: None,
                modified_at: None,
                accessed_at: None,
                changed_at: None,
                is_valid: true,
            },
        ];

        let entries = records_to_file_entries(&records, &DataSourceId("ds".to_string()));

        let root = entries
            .iter()
            .find(|entry| entry.id.0 == "mft:5")
            .expect("root MFT record should be retained");
        assert_eq!(root.name, "\\");
        assert!(root.parent_id.is_none());

        let child = entries
            .iter()
            .find(|entry| entry.id.0 == "mft:42")
            .expect("child MFT record should be retained");
        assert_eq!(
            child.parent_id.as_ref().map(|id| id.0.as_str()),
            Some("mft:5")
        );
    }

    #[test]
    fn mft_orphan_records_are_anchored_to_root() {
        let mut path_map = HashMap::new();
        path_map.insert("5".to_string(), (None, "\\".to_string(), true));
        path_map.insert(
            "42".to_string(),
            (Some("999".to_string()), "Orphan".to_string(), true),
        );

        assert_eq!(
            mft_parent_entry_id("42", Some("999"), &path_map),
            Some("mft:5".to_string())
        );
        assert_eq!(mft_parent_entry_id("5", None, &path_map), None);
    }

    #[test]
    fn mft_partition_prefixed_id_exposes_partition_index() {
        assert_eq!(mft_partition_index_from_entry_id("mft:3:42"), Some(3));
        assert_eq!(mft_partition_index_from_entry_id("mft:42"), None);
        assert_eq!(mft_partition_index_from_entry_id("uuid"), None);
    }

    #[test]
    fn safe_path_allows_normal_names() {
        assert!(safe_relative_path("config.txt").is_ok());
        assert!(safe_relative_path("data.json").is_ok());
        assert!(safe_relative_path("folder/subfolder/file.log").is_ok());
    }
}
