use super::visibility::visibility_flags_for_node;
use super::EnumerationStats;
use domain::{DataSourceId, EntryType, FileEntry, FileEntryId};
use evidence_core::{FileSystemDiagnostic, FileSystemDiagnosticKind, FileSystemReader, FsNode};
use infrastructure::constants::FILE_INSERT_BATCH_SIZE;
use persistence_sqlite::{
    repositories::{catalog_file_repo::CatalogFileRepo, file_repo::FileRepo},
    DbError, DbResult,
};
use rusqlite::Connection;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use uuid::Uuid;

// Amortize WAL commits while bounding both row count and long-path heap usage.
const CATALOG_COMMIT_BATCH_ROWS: usize = FILE_INSERT_BATCH_SIZE * 8;
const CATALOG_COMMIT_BATCH_BYTES: usize = 16 * 1024 * 1024;

pub(crate) fn replace_placeholder_root_checkpointed(
    conn: &Connection,
    placeholder_id: &FileEntryId,
    fs: &dyn FileSystemReader,
    root_name_override: Option<&str>,
    progress_fn: Option<&dyn Fn(u32)>,
    cancel_token: &AtomicBool,
) -> DbResult<EnumerationStats> {
    let root = fs
        .root()
        .map_err(|error| DbError::System(format!("Failed to read filesystem root: {error}")))?;
    reject_cancelled(cancel_token)?;
    let root_entry = promote_root(
        conn,
        placeholder_id,
        &root,
        root_name_override.unwrap_or(&root.name),
    )?;
    let partition_index = FileRepo::new(conn)
        .find_partition_index_by_id(placeholder_id)?
        .ok_or_else(|| {
            DbError::System(format!(
                "Partition placeholder '{}' is missing partition_index",
                placeholder_id.0
            ))
        })?;

    let mut state = CheckpointedEnumeration::new(root_entry.id, root.path);
    let mut writer = CatalogBatchWriter::new(partition_index);
    while let Some((parent_id, directory_path)) = state.queue.pop_front() {
        reject_cancelled(cancel_token)?;
        let children_result = fs.list_children(&directory_path);
        state.extend_diagnostics(fs.take_diagnostics(), &directory_path);
        let children = match children_result {
            Ok(children) => children,
            Err(error) => {
                state.push_diagnostic(
                    FileSystemDiagnostic::new(
                        FileSystemDiagnosticKind::DirectoryUnreadable,
                        format!("Cannot read '{directory_path}': {error}"),
                    )
                    .with_default_path(&directory_path),
                );
                continue;
            }
        };
        let entries = state.prepare_children(&root_entry.data_source_id, &parent_id, children);
        writer.extend(conn, entries, cancel_token, progress_fn)?;
    }
    state.extend_diagnostics(fs.take_diagnostics(), "");
    writer.finish(conn, cancel_token, progress_fn)?;
    Ok(state.stats)
}

fn promote_root(
    conn: &Connection,
    placeholder_id: &FileEntryId,
    root: &FsNode,
    root_name: &str,
) -> DbResult<FileEntry> {
    let transaction = conn.unchecked_transaction()?;
    let repo = FileRepo::new(&transaction);
    let mut entry = repo
        .find_by_id(placeholder_id)?
        .ok_or_else(|| DbError::System("Partition placeholder root not found".to_string()))?;
    entry.path = String::new();
    entry.name = root_name.to_string();
    entry.entry_type = EntryType::Directory;
    entry.size = None;
    entry.ext = None;
    entry.deleted = false;
    entry.hidden = root.hidden;
    entry.system = root.system;
    entry.encrypted = root.encrypted;
    entry.created_at = root.created_at;
    entry.modified_at = root.modified_at;
    entry.accessed_at = root.accessed_at;
    entry.changed_at = root.changed_at;
    entry.hash_sha256 = None;
    CatalogFileRepo::new(&transaction).update_root_in_transaction(&entry)?;
    transaction.commit()?;
    Ok(entry)
}

fn reject_cancelled(cancel_token: &AtomicBool) -> DbResult<()> {
    if cancel_token.load(Ordering::Relaxed) {
        Err(DbError::System("Enumeration cancelled".to_string()))
    } else {
        Ok(())
    }
}

fn enumeration_progress(processed: u64) -> u32 {
    let value = if processed < 100 {
        processed
    } else if processed < 1_000 {
        50 + (processed - 100) * 30 / 900
    } else {
        80 + (processed - 1_000).min(5_000) * 15 / 5_000
    };
    value as u32
}

struct CheckpointedEnumeration {
    queue: VecDeque<(FileEntryId, String)>,
    stats: EnumerationStats,
}

impl CheckpointedEnumeration {
    fn new(root_id: FileEntryId, root_path: String) -> Self {
        let mut queue = VecDeque::new();
        queue.push_back((root_id, root_path));
        Self {
            queue,
            stats: EnumerationStats {
                file_count: 0,
                dir_count: 1,
                total_size: 0,
                warnings: Vec::new(),
                diagnostics: Vec::new(),
            },
        }
    }

    fn prepare_children(
        &mut self,
        data_source_id: &DataSourceId,
        parent_id: &FileEntryId,
        children: Vec<FsNode>,
    ) -> Vec<FileEntry> {
        let mut entries = Vec::with_capacity(children.len());
        for child in children {
            if child.name == "." || child.name == ".." {
                continue;
            }
            let id = FileEntryId(Uuid::new_v4().to_string());
            if child.is_dir {
                self.stats.dir_count += 1;
                self.queue.push_back((id.clone(), child.path.clone()));
            } else {
                self.stats.file_count += 1;
                self.stats.total_size = self.stats.total_size.saturating_add(child.size);
            }
            entries.push(file_entry_for_child(data_source_id, parent_id, id, &child));
        }
        entries
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
}

struct CatalogBatchWriter {
    batch: Vec<FileEntry>,
    batch_bytes: usize,
    partition_index: usize,
    committed: u64,
}

impl CatalogBatchWriter {
    fn new(partition_index: usize) -> Self {
        Self {
            batch: Vec::with_capacity(CATALOG_COMMIT_BATCH_ROWS),
            batch_bytes: 0,
            partition_index,
            committed: 0,
        }
    }

    fn extend(
        &mut self,
        conn: &Connection,
        entries: Vec<FileEntry>,
        cancel_token: &AtomicBool,
        progress_fn: Option<&dyn Fn(u32)>,
    ) -> DbResult<()> {
        for entry in entries {
            reject_cancelled(cancel_token)?;
            let entry_bytes = estimated_entry_bytes(&entry);
            if !self.batch.is_empty()
                && (self.batch.len() >= CATALOG_COMMIT_BATCH_ROWS
                    || self.batch_bytes.saturating_add(entry_bytes) > CATALOG_COMMIT_BATCH_BYTES)
            {
                self.flush(conn, cancel_token, progress_fn)?;
            }
            self.batch.push(entry);
            self.batch_bytes = self.batch_bytes.saturating_add(entry_bytes);
            if self.batch.len() >= CATALOG_COMMIT_BATCH_ROWS
                || self.batch_bytes >= CATALOG_COMMIT_BATCH_BYTES
            {
                self.flush(conn, cancel_token, progress_fn)?;
            }
        }
        Ok(())
    }

    fn finish(
        &mut self,
        conn: &Connection,
        cancel_token: &AtomicBool,
        progress_fn: Option<&dyn Fn(u32)>,
    ) -> DbResult<()> {
        reject_cancelled(cancel_token)?;
        self.flush(conn, cancel_token, None)?;
        if let Some(progress) = progress_fn {
            progress(100);
        }
        reject_cancelled(cancel_token)
    }

    fn flush(
        &mut self,
        conn: &Connection,
        cancel_token: &AtomicBool,
        progress_fn: Option<&dyn Fn(u32)>,
    ) -> DbResult<()> {
        if self.batch.is_empty() {
            return Ok(());
        }
        reject_cancelled(cancel_token)?;
        let transaction = conn.unchecked_transaction()?;
        CatalogFileRepo::new(&transaction)
            .insert_batch_with_partition_index_in_transaction(&self.batch, self.partition_index)?;
        transaction.commit()?;
        self.committed = self.committed.saturating_add(self.batch.len() as u64);
        self.batch.clear();
        self.batch_bytes = 0;
        if let Some(progress) = progress_fn {
            progress(enumeration_progress(self.committed));
        }
        reject_cancelled(cancel_token)
    }
}

fn estimated_entry_bytes(entry: &FileEntry) -> usize {
    std::mem::size_of::<FileEntry>()
        .saturating_add(entry.id.0.len())
        .saturating_add(entry.parent_id.as_ref().map_or(0, |parent| parent.0.len()))
        .saturating_add(entry.data_source_id.0.len())
        .saturating_add(entry.path.len())
        .saturating_add(entry.name.len())
        .saturating_add(entry.ext.as_ref().map_or(0, String::len))
        .saturating_add(entry.hash_sha256.as_ref().map_or(0, String::len))
}

fn file_entry_for_child(
    data_source_id: &DataSourceId,
    parent_id: &FileEntryId,
    id: FileEntryId,
    child: &FsNode,
) -> FileEntry {
    let (hidden, system) = visibility_flags_for_node(child);
    FileEntry {
        id,
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
        ext: (!child.is_dir)
            .then(|| child.name.rsplit_once('.').map(|(_, ext)| ext.to_string()))
            .flatten(),
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
#[path = "../../tests/unit/file_service/checkpointed_enumeration.rs"]
mod tests;
