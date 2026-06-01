//! Filesystem enumeration — BFS traversal and batch insertion.
//!
//! Handles walking filesystem trees and inserting FileEntry records into SQLite.

use crate::hash_service::HashService;
use domain::{DataSourceId, EntryType, FileEntry, FileEntryId};
use evidence_core::FileSystemReader;
use infrastructure::constants::FILE_INSERT_BATCH_SIZE;
use persistence_sqlite::{repositories::file_repo::FileRepo, DbResult};
use rusqlite::Connection;
use std::collections::VecDeque;
use std::path::Path;
use uuid::Uuid;

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

    // Use a single transaction for the entire enumeration
    let tx = conn.unchecked_transaction()?;
    let repo = FileRepo::new(&tx);
    repo.insert_batch(&[root_entry])?;
    let result = walk_and_insert_children(&repo, fs, data_source_id, root_id, progress_fn);
    tx.commit()?;
    result
}

pub(crate) fn compute_enumeration_progress(processed: u64) -> u64 {
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

/// 为数据源中的文件计算 SHA-256 哈希
///
/// 遍历所有文件条目，计算哈希并更新数据库。
/// 仅对逻辑目录类型的数据源有效。
pub fn compute_hashes_for_data_source(
    conn: &Connection,
    data_source_id: &DataSourceId,
    source_root: &Path,
    progress_fn: Option<&dyn Fn(u32)>,
) -> DbResult<u64> {
    let repo = FileRepo::new(conn);
    let entries = repo.find_by_data_source(data_source_id)?;

    let file_entries: Vec<_> = entries
        .iter()
        .filter(|e| e.entry_type == EntryType::File && e.hash_sha256.is_none())
        .collect();

    let total = file_entries.len() as u64;
    let mut processed = 0u64;
    let mut computed = 0u64;

    for entry in file_entries {
        let file_path = source_root.join(&entry.path);
        match HashService::sha256_file(&file_path) {
            Ok(hash) => {
                // 更新数据库中的哈希值
                conn.execute(
                    "UPDATE file_entries SET hash_sha256 = ?1 WHERE id = ?2",
                    rusqlite::params![hash, entry.id.0],
                )?;
                computed += 1;
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to compute hash for {}: {}",
                    entry.path,
                    e
                );
            }
        }

        processed += 1;
        if let Some(ref pf) = progress_fn {
            let pct = if total > 0 {
                (processed * 100 / total) as u32
            } else {
                100
            };
            pf(pct);
        }
    }

    Ok(computed)
}
