use super::SourceStore;
use crate::{
    DatabaseConfig, FjallStorageError, KeyspaceName, PersistMode, Result, SnapshotEntry,
    SourceReadSnapshot, WriteAction,
};
use fjall::Readable;
use std::{
    ops::Bound,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};

const CLONE_PAGE_ENTRIES: usize = 4_096;
const CLONE_PAGE_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Default)]
struct CloneReadWork {
    entries: usize,
    bytes: usize,
}

impl Drop for CloneReadWork {
    fn drop(&mut self) {
        crate::instrumentation::record_scan(self.entries, self.bytes);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceCloneProgress {
    pub keyspace: Option<KeyspaceName>,
    pub completed_keyspaces: usize,
    pub total_keyspaces: usize,
    pub records_copied: u64,
    /// Logical key and value bytes copied, excluding engine bookkeeping.
    pub bytes_copied: u64,
    pub durable: bool,
}

impl SourceStore {
    /// Copies a consistent snapshot in batches of at most 4096 entries or 8 MiB.
    /// A larger individual entry is copied as one batch.
    /// The caller owns and cleans up the partial destination on error.
    pub fn clone_to_with_control(
        &self,
        path: impl AsRef<Path>,
        config: DatabaseConfig,
        cancel: &AtomicBool,
        mut progress: impl FnMut(SourceCloneProgress),
    ) -> Result<SourceStore> {
        ensure_not_cancelled(cancel)?;
        let target = SourceStore::open(path, config)?;
        let snapshot = self.database.read_snapshot();
        let mut keyspaces = self.database.keyspace_names().collect::<Vec<_>>();
        keyspaces.sort_unstable_by_key(|keyspace| keyspace.as_str());
        let mut state = SourceCloneProgress {
            keyspace: None,
            completed_keyspaces: 0,
            total_keyspaces: keyspaces.len(),
            records_copied: 0,
            bytes_copied: 0,
            durable: false,
        };
        progress(state);
        for keyspace in keyspaces {
            state.keyspace = Some(keyspace);
            copy_keyspace(&snapshot, &target, cancel, &mut state, &mut progress)?;
            state.completed_keyspaces += 1;
            progress(state);
        }
        ensure_not_cancelled(cancel)?;
        target.database.persist(PersistMode::SyncAll)?;
        state.keyspace = None;
        state.durable = true;
        progress(state);
        Ok(target)
    }
}

fn copy_keyspace(
    snapshot: &SourceReadSnapshot,
    target: &SourceStore,
    cancel: &AtomicBool,
    state: &mut SourceCloneProgress,
    progress: &mut impl FnMut(SourceCloneProgress),
) -> Result<()> {
    let keyspace = state
        .keyspace
        .expect("clone keyspace selected before copying");
    let mut cursor = None;
    loop {
        ensure_not_cancelled(cancel)?;
        let (page, bytes) = read_bounded_page(snapshot, keyspace, cursor.as_deref(), cancel)?;
        if page.is_empty() {
            return Ok(());
        }
        ensure_not_cancelled(cancel)?;
        let count = page.len() as u64;
        cursor = page.last().map(|entry| entry.key.clone());
        let actions = page
            .into_iter()
            .map(|entry| WriteAction::Put {
                keyspace,
                key: entry.key,
                value: entry.value,
            })
            .collect::<Vec<_>>();
        ensure_not_cancelled(cancel)?;
        target.database.write_batch(&actions, PersistMode::Buffer)?;
        state.records_copied = state
            .records_copied
            .checked_add(count)
            .ok_or_else(progress_overflow)?;
        state.bytes_copied = state
            .bytes_copied
            .checked_add(bytes)
            .ok_or_else(progress_overflow)?;
        progress(*state);
    }
}

pub(crate) fn read_bounded_page(
    snapshot: &SourceReadSnapshot,
    keyspace: KeyspaceName,
    cursor: Option<&[u8]>,
    cancel: &AtomicBool,
) -> Result<(Vec<SnapshotEntry>, u64)> {
    let start = cursor
        .map(|key| Bound::Excluded(key.to_vec()))
        .unwrap_or(Bound::Unbounded);
    let keyspace = snapshot
        .keyspaces
        .get(&keyspace)
        .expect("configured keyspace is present");
    let mut entries = Vec::with_capacity(CLONE_PAGE_ENTRIES);
    let mut bytes = 0u64;
    let mut read_work = CloneReadWork::default();
    for entry in snapshot
        .snapshot
        .range(keyspace, (start, Bound::<Vec<u8>>::Unbounded))
    {
        ensure_not_cancelled(cancel)?;
        let (key, value) = entry.into_inner()?;
        read_work.entries += 1;
        read_work.bytes = read_work
            .bytes
            .checked_add(key.len())
            .and_then(|bytes| bytes.checked_add(value.len()))
            .ok_or_else(progress_overflow)?;
        let next = bytes
            .checked_add(key.len() as u64)
            .and_then(|bytes| bytes.checked_add(value.len() as u64))
            .ok_or_else(progress_overflow)?;
        if !entries.is_empty() && next > CLONE_PAGE_BYTES {
            break;
        }
        bytes = next;
        entries.push(SnapshotEntry {
            key: key.to_vec(),
            value: value.to_vec(),
        });
        if entries.len() == CLONE_PAGE_ENTRIES || bytes >= CLONE_PAGE_BYTES {
            break;
        }
    }
    Ok((entries, bytes))
}

pub(crate) fn ensure_not_cancelled(cancel: &AtomicBool) -> Result<()> {
    if cancel.load(Ordering::Relaxed) {
        Err(FjallStorageError::Cancelled)
    } else {
        Ok(())
    }
}

fn progress_overflow() -> FjallStorageError {
    FjallStorageError::InvalidEnvelope("source clone progress overflowed".to_owned())
}
