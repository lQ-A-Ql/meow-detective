use crate::file_service::visibility::visibility_flags_for_node;
use domain::{DataSourceId, EntryType, FileEntry, FileEntryId};
use evidence_core::{FileSystemDiagnostic, FileSystemDiagnosticKind, FileSystemReader, FsNode};
use infrastructure::constants::FILE_INSERT_BATCH_SIZE;
use persistence_source::repositories::file_repo::FileRepo;
use persistence_sqlite::{DbError, DbResult};
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
    pub diagnostics: Vec<FileSystemDiagnostic>,
}

impl EnumerationStats {
    pub fn incomplete_catalog_diagnostic_count(&self) -> usize {
        self.diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.kind.affects_catalog_completeness())
            .count()
    }
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
    enumerate_filesystem_with_root_name_and_cancel_internal(
        conn,
        data_source_id,
        fs,
        root_name_override,
        progress_fn,
        cancel_token,
        None,
    )
}

pub fn enumerate_filesystem_with_root_name_and_cancel_with_catalog(
    conn: &Connection,
    data_source_id: &DataSourceId,
    fs: &dyn FileSystemReader,
    root_name_override: Option<&str>,
    progress_fn: Option<&dyn Fn(u32)>,
    cancel_token: Option<&AtomicBool>,
    catalog: &crate::storage_runtime::ReadWriteSourceStorageHandle,
) -> DbResult<EnumerationStats> {
    enumerate_filesystem_with_root_name_and_cancel_internal(
        conn,
        data_source_id,
        fs,
        root_name_override,
        progress_fn,
        cancel_token,
        Some(catalog),
    )
}

fn enumerate_filesystem_with_root_name_and_cancel_internal(
    conn: &Connection,
    data_source_id: &DataSourceId,
    fs: &dyn FileSystemReader,
    root_name_override: Option<&str>,
    progress_fn: Option<&dyn Fn(u32)>,
    cancel_token: Option<&AtomicBool>,
    catalog: Option<&crate::storage_runtime::ReadWriteSourceStorageHandle>,
) -> DbResult<EnumerationStats> {
    let root = fs.root().map_err(|e| {
        persistence_sqlite::DbError::System(format!("Failed to read filesystem root: {}", e))
    })?;
    if cancellation_requested(cancel_token) {
        return Err(enumeration_cancelled_error());
    }
    let root_entry = root_entry_for_filesystem(&root, data_source_id, root_name_override);
    if catalog.is_none() {
        if let Some(stats) = enumerate_native_catalog(
            conn,
            data_source_id,
            fs,
            &root_entry,
            progress_fn,
            cancel_token,
        )? {
            return Ok(stats);
        }
    }
    enumerate_sqlite_catalog(
        conn,
        data_source_id,
        fs,
        &root_entry,
        progress_fn,
        cancel_token,
        catalog,
    )
}

fn root_entry_for_filesystem(
    root: &FsNode,
    data_source_id: &DataSourceId,
    root_name_override: Option<&str>,
) -> FileEntry {
    FileEntry {
        id: FileEntryId(Uuid::new_v4().to_string()),
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
    }
}

fn enumerate_native_catalog(
    conn: &Connection,
    data_source_id: &DataSourceId,
    fs: &dyn FileSystemReader,
    root_entry: &FileEntry,
    progress_fn: Option<&dyn Fn(u32)>,
    cancel_token: Option<&AtomicBool>,
) -> DbResult<Option<EnumerationStats>> {
    let Some(store) = persistence_source::detail::open_store_for_write(conn)? else {
        return Ok(None);
    };
    let source_id = persistence_source::detail::source_id(conn)?
        .ok_or_else(|| DbError::System("source identity is missing".into()))?;
    if source_id != data_source_id.0 {
        return Err(DbError::System(
            "enumeration source identity mismatch".into(),
        ));
    }
    let repository =
        persistence_source::fjall::CatalogRecordStore::from_source_store((*store).clone());
    let root_record = persistence_source::detail::file_record(root_entry);
    let mut walker = ChildBatchWalker::new(root_entry.id.clone());
    let result = repository.replace_subtree_streamed(
        &source_id,
        &root_record,
        0,
        persistence_source::fjall::PersistMode::SyncAll,
        || {
            walker
                .next_batch(fs, data_source_id, progress_fn, cancel_token)
                .map(|batch| {
                    batch.map(|entries| {
                        entries
                            .iter()
                            .map(persistence_source::detail::file_record)
                            .collect()
                    })
                })
        },
    );
    match result {
        Ok(_) => Ok(Some(walker.into_stats())),
        Err(persistence_source::fjall::CatalogStreamError::Storage(error)) => {
            Err(DbError::System(error.to_string()))
        }
        Err(persistence_source::fjall::CatalogStreamError::Producer(error)) => Err(error),
    }
}

fn enumerate_sqlite_catalog(
    conn: &Connection,
    data_source_id: &DataSourceId,
    fs: &dyn FileSystemReader,
    root_entry: &FileEntry,
    progress_fn: Option<&dyn Fn(u32)>,
    cancel_token: Option<&AtomicBool>,
    catalog: Option<&crate::storage_runtime::ReadWriteSourceStorageHandle>,
) -> DbResult<EnumerationStats> {
    let tx = conn.unchecked_transaction()?;
    let result = {
        let repo = FileRepo::new(&tx);
        repo.insert_batch_unchecked(std::slice::from_ref(root_entry))?;
        walk_and_insert_children(
            &repo,
            fs,
            data_source_id,
            root_entry.id.clone(),
            progress_fn,
            cancel_token,
        )
    };
    match result {
        Ok(stats) => {
            if let Some(catalog) = catalog {
                crate::catalog_projection::mirror_catalog_records_from_connection(
                    &tx,
                    catalog,
                    &data_source_id.0,
                )
                .map_err(|error| DbError::System(error.to_string()))?;
                catalog
                    .persist(persistence_source::fjall::PersistMode::SyncAll)
                    .map_err(|error| DbError::System(error.to_string()))?;
                crate::catalog_projection::mark_catalog_projection_seeded(
                    catalog,
                    &data_source_id.0,
                )
                .map_err(|error| DbError::System(error.to_string()))?;
            }
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
    let mut walker = ChildBatchWalker::new(root_id);
    while let Some(batch) = walker.next_batch(fs, data_source_id, progress_fn, cancel_token)? {
        repo.insert_batch_unchecked(&batch)?;
    }
    Ok(walker.into_stats())
}

pub(crate) struct ChildBatchWalker {
    queue: VecDeque<(FileEntryId, String)>,
    pending: Option<(FileEntryId, std::vec::IntoIter<FsNode>)>,
    stats: EnumerationStats,
    total_processed: u64,
    finished: bool,
}

impl ChildBatchWalker {
    pub(crate) fn new(root_id: FileEntryId) -> Self {
        let mut queue = VecDeque::new();
        queue.push_back((root_id, String::new()));
        Self {
            queue,
            pending: None,
            stats: EnumerationStats {
                file_count: 0,
                dir_count: 1,
                total_size: 0,
                warnings: Vec::new(),
                diagnostics: Vec::new(),
            },
            total_processed: 0,
            finished: false,
        }
    }

    pub(crate) fn next_batch(
        &mut self,
        fs: &dyn FileSystemReader,
        data_source_id: &DataSourceId,
        progress_fn: Option<&dyn Fn(u32)>,
        cancel_token: Option<&AtomicBool>,
    ) -> DbResult<Option<Vec<FileEntry>>> {
        if self.finished {
            return Ok(None);
        }
        let mut batch = Vec::with_capacity(FILE_INSERT_BATCH_SIZE);
        while batch.len() < FILE_INSERT_BATCH_SIZE {
            let Some((parent_id, child)) = self.next_child(fs, cancel_token)? else {
                self.finished = true;
                if let Some(progress) = progress_fn {
                    progress(100);
                }
                break;
            };
            if child.name == "." || child.name == ".." {
                continue;
            }
            let id = FileEntryId(Uuid::new_v4().to_string());
            let entry = file_entry_for_child(data_source_id, &parent_id, &id, &child);
            if child.is_dir {
                self.stats.dir_count += 1;
                self.queue.push_back((id, child.path));
            } else {
                self.stats.file_count += 1;
                self.stats.total_size = self.stats.total_size.saturating_add(child.size);
            }
            batch.push(entry);
            self.total_processed = self.total_processed.saturating_add(1);
            if self.total_processed.is_multiple_of(100) {
                if let Some(progress) = progress_fn {
                    progress(compute_enumeration_progress(self.total_processed) as u32);
                }
            }
        }
        if cancellation_requested(cancel_token) {
            return Err(enumeration_cancelled_error());
        }
        Ok((!batch.is_empty()).then_some(batch))
    }

    fn next_child(
        &mut self,
        fs: &dyn FileSystemReader,
        cancel_token: Option<&AtomicBool>,
    ) -> DbResult<Option<(FileEntryId, FsNode)>> {
        loop {
            if cancellation_requested(cancel_token) {
                return Err(enumeration_cancelled_error());
            }
            if let Some((parent, children)) = &mut self.pending {
                if let Some(child) = children.next() {
                    return Ok(Some((parent.clone(), child)));
                }
            }
            self.pending = None;
            let Some((parent, path)) = self.queue.pop_front() else {
                return Ok(None);
            };
            let children = fs.list_children(&path);
            self.extend_diagnostics(fs.take_diagnostics(), &path);
            match children {
                Ok(children) => self.pending = Some((parent, children.into_iter())),
                Err(error) => self.push_diagnostic(
                    FileSystemDiagnostic::new(
                        FileSystemDiagnosticKind::DirectoryUnreadable,
                        format!("Cannot read '{path}': {error}"),
                    )
                    .with_default_path(&path),
                ),
            }
        }
    }

    fn extend_diagnostics(&mut self, diagnostics: Vec<FileSystemDiagnostic>, directory_path: &str) {
        for diagnostic in diagnostics {
            self.push_diagnostic(diagnostic.with_default_path(directory_path));
        }
    }

    fn push_diagnostic(&mut self, diagnostic: FileSystemDiagnostic) {
        self.stats.warnings.push(diagnostic.message.clone());
        self.stats.diagnostics.push(diagnostic);
    }

    pub(crate) fn into_stats(self) -> EnumerationStats {
        self.stats
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

#[cfg(test)]
#[path = "../../tests/unit/file_service/enumeration_batches.rs"]
mod tests;
