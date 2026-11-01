use crate::file_service::visibility::visibility_flags_for_node;
use crossbeam_channel::{bounded, unbounded, Receiver, Sender};
use domain::{DataSourceId, EntryType, FileEntry, FileEntryId};
use evidence_core::{FileSystemDiagnostic, FileSystemReader, FsNode};
use infrastructure::constants::FILE_INSERT_BATCH_SIZE;
use persistence_sqlite::{repositories::file_repo::FileRepo, DbError, DbResult};
use rusqlite::Connection;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use uuid::Uuid;

struct DirectoryWork {
    parent_id: FileEntryId,
    path: String,
}

struct DirectoryResult {
    parent_id: FileEntryId,
    path: String,
    children: Vec<FsNode>,
    diagnostics: Vec<FileSystemDiagnostic>,
}

pub fn enumerate_filesystem_parallel_with_factory(
    conn: &Connection,
    data_source_id: &DataSourceId,
    worker_count: usize,
    reader_factory: Arc<dyn Fn() -> DbResult<Box<dyn FileSystemReader>> + Send + Sync>,
    root_name_override: Option<&str>,
    progress_fn: Option<&dyn Fn(u32)>,
    cancel_token: Option<&AtomicBool>,
) -> DbResult<crate::file_service::EnumerationStats> {
    let root_reader = reader_factory()?;
    let root = root_reader
        .root()
        .map_err(|error| DbError::System(format!("Failed to read filesystem root: {error}")))?;
    drop(root_reader);
    if cancellation_requested(cancel_token) {
        return Err(DbError::System("Enumeration cancelled".to_string()));
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
        encrypted: root.encrypted,
        read_only: root.read_only,
        archive: root.archive,
        unix_mode: root.unix_mode,
        created_at: root.created_at,
        modified_at: root.modified_at,
        accessed_at: root.accessed_at,
        changed_at: root.changed_at,
        hash_sha256: None,
    };

    let worker_count = worker_count.max(1);
    let (work_tx, work_rx) = unbounded::<DirectoryWork>();
    let (result_tx, result_rx) = bounded::<Result<DirectoryResult, String>>(worker_count.max(1));
    let transaction = conn.unchecked_transaction()?;
    let repo = FileRepo::new(&transaction);
    repo.insert_batch_unchecked(&[root_entry])?;
    work_tx
        .send(DirectoryWork {
            parent_id: root_id,
            path: String::new(),
        })
        .map_err(|_| DbError::System("Parallel filesystem work queue closed".to_string()))?;

    let result = std::thread::scope(|scope| {
        for _ in 0..worker_count {
            let work_rx = work_rx.clone();
            let result_tx = result_tx.clone();
            let factory = Arc::clone(&reader_factory);
            scope.spawn(move || {
                let reader = match factory() {
                    Ok(reader) => reader,
                    Err(error) => {
                        let _ = result_tx.send(Err(error.to_string()));
                        return;
                    }
                };
                while let Ok(work) = work_rx.recv() {
                    let children = reader
                        .list_children(&work.path)
                        .map_err(|error| error.to_string());
                    let diagnostics = reader.take_diagnostics();
                    if result_tx
                        .send(children.map(|children| DirectoryResult {
                            parent_id: work.parent_id,
                            path: work.path,
                            children,
                            diagnostics,
                        }))
                        .is_err()
                    {
                        return;
                    }
                }
            });
        }
        drop(result_tx);
        let result = enumerate_results(
            &repo,
            data_source_id,
            &work_tx,
            &result_rx,
            progress_fn,
            cancel_token,
        );
        drop(work_tx);
        result
    });
    let stats = result?;
    transaction.commit()?;
    Ok(stats)
}

fn enumerate_results(
    repo: &FileRepo<'_>,
    data_source_id: &DataSourceId,
    work_tx: &Sender<DirectoryWork>,
    result_rx: &Receiver<Result<DirectoryResult, String>>,
    progress_fn: Option<&dyn Fn(u32)>,
    cancel_token: Option<&AtomicBool>,
) -> DbResult<crate::file_service::EnumerationStats> {
    let mut pending = 1usize;
    let mut batch = Vec::with_capacity(FILE_INSERT_BATCH_SIZE);
    let mut processed = 0u64;
    let mut files = 0u64;
    let mut dirs = 1u64;
    let mut total_size = 0u64;
    let mut warnings = Vec::new();
    let mut diagnostics = Vec::new();
    while pending > 0 {
        if cancellation_requested(cancel_token) {
            return Err(DbError::System("Enumeration cancelled".to_string()));
        }
        let result = result_rx
            .recv()
            .map_err(|_| DbError::System("Parallel filesystem workers stopped".to_string()))?
            .map_err(DbError::System)?;
        pending = pending.saturating_sub(1);
        for diagnostic in result.diagnostics {
            tracing::warn!(path = %result.path, message = %diagnostic.message, "Filesystem enumeration diagnostic");
            warnings.push(diagnostic.message.clone());
            diagnostics.push(diagnostic);
        }
        for child in result.children {
            if child.name == "." || child.name == ".." {
                continue;
            }
            let id = FileEntryId(Uuid::new_v4().to_string());
            if child.is_dir {
                pending = pending.saturating_add(1);
                dirs = dirs.saturating_add(1);
                work_tx
                    .send(DirectoryWork {
                        parent_id: id.clone(),
                        path: child.path.clone(),
                    })
                    .map_err(|_| {
                        DbError::System("Parallel filesystem work queue closed".to_string())
                    })?;
            } else {
                files = files.saturating_add(1);
                total_size = total_size.saturating_add(child.size);
            }
            batch.push(file_entry_for_child(
                data_source_id,
                &result.parent_id,
                &id,
                &child,
            ));
            processed = processed.saturating_add(1);
            if batch.len() >= FILE_INSERT_BATCH_SIZE {
                repo.insert_batch_unchecked(&batch)?;
                batch.clear();
            }
            if processed.is_multiple_of(100) {
                if let Some(progress_fn) = progress_fn {
                    progress_fn(compute_enumeration_progress(processed));
                }
            }
        }
    }
    if !batch.is_empty() {
        repo.insert_batch_unchecked(&batch)?;
    }
    if let Some(progress_fn) = progress_fn {
        progress_fn(100);
    }
    Ok(crate::file_service::EnumerationStats {
        file_count: files,
        dir_count: dirs,
        total_size,
        warnings,
        diagnostics,
    })
}

fn cancellation_requested(cancel_token: Option<&AtomicBool>) -> bool {
    cancel_token.is_some_and(|token| token.load(Ordering::Relaxed))
}

fn compute_enumeration_progress(processed: u64) -> u32 {
    if processed < 100 {
        processed as u32
    } else if processed < 1000 {
        (50 + (processed - 100) * 30 / 900) as u32
    } else {
        (80 + (processed - 1000).min(5000) * 15 / 5000) as u32
    }
}

fn file_entry_for_child(
    data_source_id: &DataSourceId,
    parent_id: &FileEntryId,
    id: &FileEntryId,
    child: &FsNode,
) -> FileEntry {
    let (hidden, system) = visibility_flags_for_node(child);
    FileEntry {
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
        size: (!child.is_dir).then_some(child.size),
        ext: child
            .name
            .rsplit('.')
            .next()
            .filter(|extension| *extension != child.name)
            .map(str::to_string),
        deleted: false,
        hidden,
        system,
        encrypted: child.encrypted,
        read_only: child.read_only,
        archive: child.archive,
        unix_mode: child.unix_mode,
        created_at: child.created_at,
        modified_at: child.modified_at,
        accessed_at: child.accessed_at,
        changed_at: child.changed_at,
        hash_sha256: None,
    }
}
