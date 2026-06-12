use crate::file_service::visibility::visibility_flags_for_node;
use domain::{DataSourceId, EntryType, FileEntry, FileEntryId};
use evidence_core::FileSystemReader;
use infrastructure::constants::FILE_INSERT_BATCH_SIZE;
use persistence_sqlite::{repositories::file_repo::FileRepo, DbError, DbResult};
use rusqlite::Connection;
use std::{
    collections::VecDeque,
    sync::atomic::{AtomicBool, Ordering},
};
use uuid::Uuid;

pub struct EnumerationStats {
    pub file_count: u64,
    pub dir_count: u64,
    pub total_size: u64,
    pub warnings: Vec<String>,
}

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
    enumerate_filesystem_with_root_name_and_cancel(
        conn,
        data_source_id,
        fs,
        root_name_override,
        progress_fn,
        None,
    )
}

pub fn enumerate_filesystem_with_root_name_and_cancel(
    conn: &Connection,
    data_source_id: &DataSourceId,
    fs: &dyn FileSystemReader,
    root_name_override: Option<&str>,
    progress_fn: Option<&dyn Fn(u32)>,
    cancel_token: Option<&AtomicBool>,
) -> DbResult<EnumerationStats> {
    let root = fs.root().map_err(|e| {
        persistence_sqlite::DbError::System(format!("Failed to read filesystem root: {}", e))
    })?;

    if cancellation_requested(cancel_token) {
        return Err(persistence_sqlite::DbError::System(
            "Enumeration cancelled".to_string(),
        ));
    }

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
        hidden: root.hidden,
        system: root.system,
        created_at: root.created_at,
        modified_at: root.modified_at,
        accessed_at: root.accessed_at,
        changed_at: None,
        hash_sha256: None,
    };

    let tx = conn.unchecked_transaction()?;
    let result = {
        let repo = FileRepo::new(&tx);
        repo.insert_batch_unchecked(&[root_entry])?;
        walk_and_insert_children(
            &repo,
            fs,
            data_source_id,
            root_id,
            progress_fn,
            cancel_token,
        )
    };
    match result {
        Ok(stats) => {
            tx.commit()?;
            Ok(stats)
        }
        Err(error) => {
            tx.rollback().ok();
            Err(error)
        }
    }
}

fn cancellation_requested(cancel_token: Option<&AtomicBool>) -> bool {
    cancel_token
        .map(|token| token.load(Ordering::Relaxed))
        .unwrap_or(false)
}

fn enumeration_cancelled_error() -> DbError {
    DbError::System("Enumeration cancelled".to_string())
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

pub(crate) fn walk_and_insert_children(
    repo: &FileRepo<'_>,
    fs: &dyn FileSystemReader,
    data_source_id: &DataSourceId,
    root_id: FileEntryId,
    progress_fn: Option<&dyn Fn(u32)>,
    cancel_token: Option<&AtomicBool>,
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
        if cancellation_requested(cancel_token) {
            return Err(enumeration_cancelled_error());
        }

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
            if cancellation_requested(cancel_token) {
                return Err(enumeration_cancelled_error());
            }

            if child.name == "." || child.name == ".." {
                continue;
            }

            let id = FileEntryId(Uuid::new_v4().to_string());
            let (hidden, system) = visibility_flags_for_node(&child);
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
                hidden,
                system,
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
                repo.insert_batch_unchecked(&batch)?;
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
        repo.insert_batch_unchecked(&batch)?;
    }

    if let Some(ref pf) = progress_fn {
        pf(100);
    }
    if cancellation_requested(cancel_token) {
        return Err(enumeration_cancelled_error());
    }

    Ok(stats)
}
