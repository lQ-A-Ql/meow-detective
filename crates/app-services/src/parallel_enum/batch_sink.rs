use super::progress::ENUM_PROGRESS_INTERVAL;
use evidence_core::{FileSystemReader, FsNode};
use fs_ntfs::mft_scanner::MftRecord;
use rusqlite::{CachedStatement, Connection};
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Debug, Clone, Copy, Default)]
pub(super) struct EnumerationStats {
    pub(super) file_count: u64,
    pub(super) dir_count: u64,
    pub(super) total_size: u64,
    pub(super) directory_index_failures: u64,
}

impl EnumerationStats {
    fn record(&mut self, node: &FsNode) {
        if node.is_dir {
            self.dir_count += 1;
        } else {
            self.file_count += 1;
            self.total_size += node.size;
        }
    }

    fn entries(self) -> u64 {
        self.file_count + self.dir_count
    }
}

pub(super) fn clear_staging_file_entries(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch("DELETE FROM file_entries")
}

/// Enumerate one filesystem with serial evidence reads and one SQLite writer.
pub(super) fn enumerate_fs_to_staging(
    conn: &Connection,
    fs: &dyn FileSystemReader,
    data_source_id: &str,
    partition_index: usize,
    cancel_token: &AtomicBool,
    progress_cb: Option<&dyn Fn(u64, u64)>,
) -> Result<EnumerationStats, String> {
    check_cancelled(cancel_token)?;
    let root = fs.root().map_err(|error| error.to_string())?;
    check_cancelled(cancel_token)?;
    let roots = sorted_nodes(
        fs.list_children(&root.path)
            .map_err(|error| error.to_string())?,
    );
    check_cancelled(cancel_token)?;

    conn.execute_batch("BEGIN TRANSACTION")
        .map_err(|error| format!("Begin transaction: {error}"))?;
    let result = enumerate_transaction(
        conn,
        fs,
        data_source_id,
        partition_index,
        roots,
        cancel_token,
        progress_cb,
    );
    if result.is_err() {
        conn.execute_batch("ROLLBACK").ok();
    }
    result
}

fn enumerate_transaction(
    conn: &Connection,
    fs: &dyn FileSystemReader,
    data_source_id: &str,
    partition_index: usize,
    roots: Vec<FsNode>,
    cancel_token: &AtomicBool,
    progress_cb: Option<&dyn Fn(u64, u64)>,
) -> Result<EnumerationStats, String> {
    let mut statement = prepare_insert(conn)?;
    let mut stack = vec![(roots, None)];
    let mut stats = EnumerationStats {
        // The staging rows are folded under one persisted partition root at merge time.
        dir_count: 1,
        ..EnumerationStats::default()
    };

    while let Some((entries, parent_id)) = stack.pop() {
        check_cancelled(cancel_token)?;
        enumerate_batch(
            fs,
            &mut statement,
            data_source_id,
            partition_index,
            entries,
            parent_id,
            &mut stack,
            &mut stats,
            cancel_token,
            progress_cb,
        )?;
    }

    drop(statement);
    conn.execute_batch("COMMIT")
        .map_err(|error| format!("Commit error: {error}"))?;
    Ok(stats)
}

#[allow(clippy::too_many_arguments)]
fn enumerate_batch(
    fs: &dyn FileSystemReader,
    statement: &mut CachedStatement<'_>,
    data_source_id: &str,
    partition_index: usize,
    entries: Vec<FsNode>,
    parent_id: Option<String>,
    stack: &mut Vec<(Vec<FsNode>, Option<String>)>,
    stats: &mut EnumerationStats,
    cancel_token: &AtomicBool,
    progress_cb: Option<&dyn Fn(u64, u64)>,
) -> Result<(), String> {
    for entry in entries {
        check_cancelled(cancel_token)?;
        let entry_id = uuid::Uuid::new_v4().to_string();
        insert_node(
            statement,
            data_source_id,
            partition_index,
            parent_id.as_deref(),
            &entry,
            &entry_id,
        )?;
        stats.record(&entry);
        report_progress(*stats, progress_cb);

        if entry.is_dir {
            check_cancelled(cancel_token)?;
            match fs.list_children(&entry.path) {
                Ok(children) => stack.push((sorted_nodes(children), Some(entry_id))),
                Err(error) => tracing::warn!("Failed to list {}: {}", entry.path, error),
            }
        }
    }
    Ok(())
}

fn prepare_insert(conn: &Connection) -> Result<CachedStatement<'_>, String> {
    conn.prepare_cached(
        "INSERT INTO file_entries
         (id, parent_id, data_source_id, path, name, entry_type,
          size, ext, deleted, hidden, system, read_only, archive, encrypted, created_at, modified_at, accessed_at, changed_at, hash_sha256,
          partition_index)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)",
    )
    .map_err(|error| format!("Prepare error: {error}"))
}

fn insert_node(
    statement: &mut CachedStatement<'_>,
    data_source_id: &str,
    partition_index: usize,
    parent_id: Option<&str>,
    node: &FsNode,
    entry_id: &str,
) -> Result<(), String> {
    let (hidden, system) = visibility_flags_for_name(&node.name, node.hidden, node.system);
    statement
        .execute(rusqlite::params![
            entry_id,
            parent_id,
            data_source_id,
            node.path,
            node.name,
            if node.is_dir { "directory" } else { "file" },
            Some(node.size),
            file_extension(node),
            0i32,
            hidden as i32,
            system as i32,
            node.read_only as i32,
            node.archive as i32,
            node.encrypted as i32,
            node.created_at.as_ref().map(|value| value.to_rfc3339()),
            node.modified_at.as_ref().map(|value| value.to_rfc3339()),
            node.accessed_at.as_ref().map(|value| value.to_rfc3339()),
            node.changed_at.as_ref().map(|value| value.to_rfc3339()),
            None::<String>,
            partition_index as i64,
        ])
        .map_err(|error| format!("Insert error: {error}"))?;
    Ok(())
}

fn file_extension(node: &FsNode) -> Option<String> {
    node.name.rsplit('.').next().map(str::to_lowercase)
}

fn sorted_nodes(mut nodes: Vec<FsNode>) -> Vec<FsNode> {
    nodes.sort_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then_with(|| left.name.cmp(&right.name))
            .then_with(|| right.is_dir.cmp(&left.is_dir))
    });
    nodes
}

fn report_progress(stats: EnumerationStats, progress_cb: Option<&dyn Fn(u64, u64)>) {
    if stats.entries().is_multiple_of(ENUM_PROGRESS_INTERVAL) {
        if let Some(callback) = progress_cb {
            callback(stats.entries(), stats.total_size);
        }
    }
}

fn check_cancelled(cancel_token: &AtomicBool) -> Result<(), String> {
    if cancel_token.load(Ordering::Relaxed) {
        Err("Cancelled".to_string())
    } else {
        Ok(())
    }
}

pub(super) fn visibility_flags_for_name(name: &str, hidden: bool, system: bool) -> (bool, bool) {
    let inferred_system = matches!(
        name.to_ascii_lowercase().as_str(),
        "$recycle.bin"
            | "system volume information"
            | "pagefile.sys"
            | "hiberfil.sys"
            | "swapfile.sys"
    );
    (
        hidden || name.starts_with('.') || inferred_system,
        system || inferred_system,
    )
}

pub(super) fn prepare_mft_insert(conn: &Connection) -> rusqlite::Result<CachedStatement<'_>> {
    conn.prepare_cached(
        "INSERT OR IGNORE INTO file_entries
         (id, parent_id, data_source_id, path, name, entry_type,
          size, ext, deleted, hidden, system, read_only, archive, encrypted, created_at, modified_at, accessed_at, changed_at, hash_sha256,
          partition_index)
         VALUES (?1, ?2, ?3, '', ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, NULL, ?18)",
    )
}

pub(super) fn stage_mft_record(
    statement: &mut CachedStatement<'_>,
    record: &MftRecord,
    data_source_id: &str,
    partition_index: usize,
) -> Result<(Option<String>, String), String> {
    let name = if record.record_number == 5 && (record.name.is_empty() || record.name == ".") {
        "\\".to_string()
    } else {
        record.name.clone()
    };
    let parent_key = (record.record_number != 5).then(|| record.parent_ref.to_string());
    let parent_id = parent_key
        .as_deref()
        .map(|parent| mft_entry_id_from_key(partition_index, parent));
    let (hidden, system) = visibility_flags_for_name(&name, record.hidden, record.system);
    statement
        .execute(rusqlite::params![
            mft_entry_id(partition_index, record.record_number),
            parent_id,
            data_source_id,
            name,
            if record.is_dir { "directory" } else { "file" },
            (!record.is_dir).then_some(record.size),
            mft_record_extension(record),
            record.deleted as i32,
            hidden as i32,
            system as i32,
            record.read_only as i32,
            record.archive as i32,
            record.encrypted as i32,
            record.created_at.as_ref().map(|value| value.to_rfc3339()),
            record.modified_at.as_ref().map(|value| value.to_rfc3339()),
            record.accessed_at.as_ref().map(|value| value.to_rfc3339()),
            record.changed_at.as_ref().map(|value| value.to_rfc3339()),
            partition_index as i64,
        ])
        .map_err(|error| format!("Insert MFT staging row: {error}"))?;
    Ok((parent_key, name))
}

pub(super) fn prepare_ntfs_index_insert(conn: &Connection) -> Result<CachedStatement<'_>, String> {
    conn.prepare_cached(
        "INSERT OR IGNORE INTO file_entries
         (id, parent_id, data_source_id, path, name, entry_type,
          size, ext, deleted, hidden, system, read_only, archive, encrypted, created_at, modified_at, accessed_at, changed_at, hash_sha256,
          partition_index)
         VALUES (?1, ?2, ?3, '', ?4, ?5, ?6, ?7, 0, ?8, ?9, ?10, ?11, ?12, NULL, NULL, NULL, NULL, NULL, ?13)",
    )
    .map_err(|error| format!("Prepare NTFS directory index backfill: {error}"))
}

#[allow(clippy::too_many_arguments)]
pub(super) fn insert_ntfs_index_entry(
    statement: &mut CachedStatement<'_>,
    data_source_id: &str,
    partition_index: usize,
    parent_ref: u64,
    mft_ref: u64,
    name: &str,
    is_dir: bool,
    size: u64,
    hidden: bool,
    system: bool,
    read_only: bool,
    archive: bool,
    encrypted: bool,
) -> Result<usize, String> {
    let (hidden, system) = visibility_flags_for_name(name, hidden, system);
    statement
        .execute(rusqlite::params![
            mft_entry_id(partition_index, mft_ref),
            mft_entry_id(partition_index, parent_ref),
            data_source_id,
            name,
            if is_dir { "directory" } else { "file" },
            (!is_dir).then_some(size),
            (!is_dir).then(|| extension_from_name(name)).flatten(),
            hidden as i32,
            system as i32,
            read_only as i32,
            archive as i32,
            encrypted as i32,
            partition_index as i64,
        ])
        .map_err(|error| format!("Insert NTFS directory index backfill row: {error}"))
}

pub(super) fn mft_entry_id(partition_index: usize, record_number: u64) -> String {
    format!("mft:{partition_index}:{record_number}")
}

pub(super) fn mft_entry_id_from_key(partition_index: usize, record_key: &str) -> String {
    format!("mft:{partition_index}:{record_key}")
}

fn mft_record_extension(record: &MftRecord) -> Option<String> {
    if record.is_dir {
        None
    } else {
        extension_from_name(&record.name)
    }
}

fn extension_from_name(name: &str) -> Option<String> {
    name.rsplit('.')
        .next()
        .filter(|extension| *extension != name)
        .map(str::to_string)
}
