use crossbeam_channel::{Receiver, Sender};
use domain::DataSourceId;
use evidence_core::{FileSystemDiagnostic, FileSystemDiagnosticKind, FileSystemReader};
use infrastructure::constants::FILE_INSERT_BATCH_SIZE;
use persistence_source::repositories::file_repo::FileRepo;
use persistence_sqlite::{DbError, DbResult};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use super::producer::{
    file_entry_for_child, DirectoryResult, DirectoryWork, WorkerResult, MAX_ENUMERATION_DEPTH,
    MAX_ENUMERATION_PATH_BYTES,
};

pub(super) struct WorkerRuntime<'a> {
    pub(super) reader_factory: Arc<dyn Fn() -> DbResult<Box<dyn FileSystemReader>> + Send + Sync>,
    pub(super) work_tx: Sender<DirectoryWork>,
    pub(super) work_rx: Receiver<DirectoryWork>,
    pub(super) result_tx: Sender<WorkerResult>,
    pub(super) result_rx: Receiver<WorkerResult>,
    pub(super) progress_fn: Option<&'a dyn Fn(u32)>,
    pub(super) cancel_token: Option<&'a AtomicBool>,
    pub(super) stop: Arc<AtomicBool>,
}

pub(super) fn run_workers<'a>(
    repo: &FileRepo<'_>,
    data_source_id: &DataSourceId,
    worker_count: usize,
    runtime: WorkerRuntime<'a>,
) -> DbResult<crate::file_service::EnumerationStats> {
    let WorkerRuntime {
        reader_factory,
        work_tx,
        work_rx,
        result_tx,
        result_rx,
        progress_fn,
        cancel_token,
        stop,
    } = runtime;
    std::thread::scope(|scope| {
        for _ in 0..worker_count {
            spawn_worker(
                scope,
                work_rx.clone(),
                result_tx.clone(),
                Arc::clone(&reader_factory),
                Arc::clone(&stop),
            );
        }
        drop(result_tx);
        let result = enumerate_results(
            repo,
            data_source_id,
            &work_tx,
            &result_rx,
            progress_fn,
            cancel_token,
            &stop,
        );
        if result.is_err() {
            stop.store(true, Ordering::Release);
        }
        drop(work_tx);
        result
    })
}

pub(super) fn spawn_worker<'scope, 'env>(
    scope: &'scope std::thread::Scope<'scope, 'env>,
    work_rx: Receiver<DirectoryWork>,
    result_tx: Sender<WorkerResult>,
    factory: Arc<dyn Fn() -> DbResult<Box<dyn FileSystemReader>> + Send + Sync>,
    stop: Arc<AtomicBool>,
) {
    scope.spawn(move || {
        let reader = match factory() {
            Ok(reader) => reader,
            Err(error) => {
                stop.store(true, Ordering::Release);
                let _ = result_tx.send(WorkerResult::Fatal(error.to_string()));
                return;
            }
        };
        while let Ok(work) = work_rx.recv() {
            if stop.load(Ordering::Acquire) {
                return;
            }
            let children = reader.list_children(&work.path).map_err(|error| {
                FileSystemDiagnostic::new(
                    FileSystemDiagnosticKind::DirectoryUnreadable,
                    format!("Cannot read '{}': {}", work.path, error),
                )
                .with_default_path(&work.path)
            });
            let mut diagnostics = reader
                .take_diagnostics()
                .into_iter()
                .map(|diagnostic| diagnostic.with_default_path(&work.path))
                .collect::<Vec<_>>();
            let children = match children {
                Ok(children) => children,
                Err(diagnostic) => {
                    diagnostics.push(diagnostic);
                    Vec::new()
                }
            };
            if result_tx
                .send(WorkerResult::Directory(DirectoryResult {
                    parent_id: work.parent_id,
                    path: work.path,
                    depth: work.depth,
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

fn enumerate_results(
    repo: &FileRepo<'_>,
    data_source_id: &DataSourceId,
    work_tx: &Sender<DirectoryWork>,
    result_rx: &Receiver<WorkerResult>,
    progress_fn: Option<&dyn Fn(u32)>,
    cancel_token: Option<&AtomicBool>,
    stop: &Arc<AtomicBool>,
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
            stop.store(true, Ordering::Release);
            return Err(DbError::System("Enumeration cancelled".to_string()));
        }
        let result = result_rx.recv().map_err(|_| {
            stop.store(true, Ordering::Release);
            DbError::System("Parallel filesystem workers stopped".to_string())
        })?;
        let result = match result {
            WorkerResult::Directory(result) => result,
            WorkerResult::Fatal(error) => {
                stop.store(true, Ordering::Release);
                return Err(DbError::System(error));
            }
        };
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
            let id = domain::FileEntryId(uuid::Uuid::new_v4().to_string());
            if child.is_dir {
                let child_depth = checked_child_depth(result.depth, &child.path)?;
                pending = pending.saturating_add(1);
                dirs = dirs.saturating_add(1);
                work_tx
                    .send(DirectoryWork {
                        parent_id: id.clone(),
                        path: child.path.clone(),
                        depth: child_depth,
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
    if cancellation_requested(cancel_token) {
        stop.store(true, Ordering::Release);
        return Err(DbError::System("Enumeration cancelled".to_string()));
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

pub(super) fn cancellation_requested(cancel_token: Option<&AtomicBool>) -> bool {
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

fn checked_child_depth(parent_depth: u32, path: &str) -> DbResult<u32> {
    let depth = parent_depth.saturating_add(1);
    if depth > MAX_ENUMERATION_DEPTH || path.len() > MAX_ENUMERATION_PATH_BYTES {
        return Err(DbError::System(format!(
            "Filesystem directory traversal exceeded safety limits at '{path}'"
        )));
    }
    Ok(depth)
}
