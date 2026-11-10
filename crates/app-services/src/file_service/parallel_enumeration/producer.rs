use crate::file_service::visibility::visibility_flags_for_node;
use crossbeam_channel::{Receiver, Sender};
use domain::{DataSourceId, EntryType, FileEntry, FileEntryId};
use evidence_core::{FileSystemDiagnostic, FsNode};
use infrastructure::constants::FILE_INSERT_BATCH_SIZE;
use persistence_sqlite::{DbError, DbResult};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use uuid::Uuid;

use super::workers::cancellation_requested;

pub(super) const MAX_ENUMERATION_DEPTH: u32 = 256;
pub(super) const MAX_ENUMERATION_PATH_BYTES: usize = 60 * 1024;

pub(super) struct DirectoryWork {
    pub(super) parent_id: FileEntryId,
    pub(super) path: String,
    pub(super) depth: u32,
}

pub(super) struct DirectoryResult {
    pub(super) parent_id: FileEntryId,
    pub(super) path: String,
    pub(super) depth: u32,
    pub(super) children: Vec<FsNode>,
    pub(super) diagnostics: Vec<FileSystemDiagnostic>,
}

pub(super) enum WorkerResult {
    Directory(DirectoryResult),
    Fatal(String),
}

pub(super) struct ParallelBatchProducer<'a> {
    work_tx: Option<Sender<DirectoryWork>>,
    result_rx: Receiver<WorkerResult>,
    progress_fn: Option<&'a dyn Fn(u32)>,
    cancel_token: Option<&'a AtomicBool>,
    stop: Arc<AtomicBool>,
    pending: usize,
    processed: u64,
    files: u64,
    dirs: u64,
    total_size: u64,
    warnings: Vec<String>,
    diagnostics: Vec<FileSystemDiagnostic>,
    finished: bool,
}

impl<'a> ParallelBatchProducer<'a> {
    pub(super) fn new(
        work_tx: Sender<DirectoryWork>,
        result_rx: Receiver<WorkerResult>,
        progress_fn: Option<&'a dyn Fn(u32)>,
        cancel_token: Option<&'a AtomicBool>,
        stop: Arc<AtomicBool>,
    ) -> Self {
        Self {
            work_tx: Some(work_tx),
            result_rx,
            progress_fn,
            cancel_token,
            stop,
            pending: 1,
            processed: 0,
            files: 0,
            dirs: 1,
            total_size: 0,
            warnings: Vec::new(),
            diagnostics: Vec::new(),
            finished: false,
        }
    }

    pub(super) fn next_batch(
        &mut self,
        data_source_id: &DataSourceId,
    ) -> DbResult<Option<Vec<FileEntry>>> {
        if self.finished {
            return Ok(None);
        }
        let mut batch = Vec::with_capacity(FILE_INSERT_BATCH_SIZE);
        while batch.len() < FILE_INSERT_BATCH_SIZE {
            if cancellation_requested(self.cancel_token) {
                self.stop.store(true, Ordering::Release);
                return Err(DbError::System("Enumeration cancelled".to_string()));
            }
            if self.pending == 0 {
                self.finished = true;
                self.work_tx.take();
                if let Some(progress_fn) = self.progress_fn {
                    progress_fn(100);
                }
                break;
            }
            let result = self.result_rx.recv().map_err(|_| {
                self.stop.store(true, Ordering::Release);
                DbError::System("Parallel filesystem workers stopped".to_string())
            })?;
            let result = match result {
                WorkerResult::Directory(result) => result,
                WorkerResult::Fatal(error) => {
                    self.stop.store(true, Ordering::Release);
                    return Err(DbError::System(error));
                }
            };
            self.pending = self.pending.saturating_sub(1);
            for diagnostic in result.diagnostics {
                tracing::warn!(path = %result.path, message = %diagnostic.message, "Filesystem enumeration diagnostic");
                self.warnings.push(diagnostic.message.clone());
                self.diagnostics.push(diagnostic);
            }
            for child in result.children {
                if child.name == "." || child.name == ".." {
                    continue;
                }
                let id = FileEntryId(Uuid::new_v4().to_string());
                if child.is_dir {
                    let child_depth = checked_child_depth(result.depth, &child.path)?;
                    self.pending = self.pending.saturating_add(1);
                    self.dirs = self.dirs.saturating_add(1);
                    self.work_tx
                        .as_ref()
                        .expect("parallel work queue remains open while work is pending")
                        .send(DirectoryWork {
                            parent_id: id.clone(),
                            path: child.path.clone(),
                            depth: child_depth,
                        })
                        .map_err(|_| {
                            DbError::System("Parallel filesystem work queue closed".to_string())
                        })?;
                } else {
                    self.files = self.files.saturating_add(1);
                    self.total_size = self.total_size.saturating_add(child.size);
                }
                batch.push(file_entry_for_child(
                    data_source_id,
                    &result.parent_id,
                    &id,
                    &child,
                ));
                self.processed = self.processed.saturating_add(1);
                if self.processed.is_multiple_of(100) {
                    if let Some(progress_fn) = self.progress_fn {
                        progress_fn(compute_enumeration_progress(self.processed));
                    }
                }
                if batch.len() == FILE_INSERT_BATCH_SIZE {
                    break;
                }
            }
        }
        if cancellation_requested(self.cancel_token) {
            self.stop.store(true, Ordering::Release);
            return Err(DbError::System("Enumeration cancelled".to_string()));
        }
        Ok((!batch.is_empty()).then_some(batch))
    }

    pub(super) fn into_stats(self) -> crate::file_service::EnumerationStats {
        crate::file_service::EnumerationStats {
            file_count: self.files,
            dir_count: self.dirs,
            total_size: self.total_size,
            warnings: self.warnings,
            diagnostics: self.diagnostics,
        }
    }
}

pub(super) fn file_entry_for_child(
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
