use super::{adjust_count_deltas, count_schema_marker_action, COUNT_FAMILY};
use crate::{
    database::WriteStream, encode_key, FjallStorageError, KeyPart, KeyspaceName, PersistMode,
    Result, ScanDirection, SourceReadSnapshot, TimelineRecordStore, WriteAction,
};
use std::{
    collections::HashMap,
    sync::atomic::{AtomicBool, Ordering},
};

const PAGE_SIZE: usize = 512;

impl TimelineRecordStore {
    /// Rebuilds count indexes from one snapshot and publishes them atomically.
    /// Progress is reported outside the write transaction after each record page.
    pub fn rebuild_count_indexes_with_control(
        &self,
        source_id: &str,
        mode: PersistMode,
        cancel: &AtomicBool,
        mut progress: impl FnMut(u64),
    ) -> Result<()> {
        ensure_not_cancelled(cancel)?;
        let snapshot = self.read_snapshot();
        let revision = self.cursor_revision(&snapshot)?;
        let deltas = collect_counts(self, &snapshot, source_id, cancel, &mut progress)?;
        ensure_not_cancelled(cancel)?;
        self.source_store
            .database()
            .write_streamed(mode, |current, stream| {
                ensure_not_cancelled(cancel)?;
                if self.cursor_revision(current)? != revision {
                    return Err(FjallStorageError::InvalidEnvelope(
                        "timeline changed during count index rebuild".to_owned(),
                    ));
                }
                clear_counts(current, stream, source_id, cancel)?;
                for (key, count) in deltas {
                    ensure_not_cancelled(cancel)?;
                    let count = u64::try_from(count).map_err(|_| {
                        FjallStorageError::InvalidEnvelope("timeline count is negative".to_owned())
                    })?;
                    stream.put(
                        KeyspaceName::TimelineIndexes,
                        key,
                        count.to_be_bytes().to_vec(),
                    );
                }
                stream_action(stream, count_schema_marker_action(source_id)?);
                let next = revision.checked_add(1).ok_or_else(|| {
                    FjallStorageError::InvalidEnvelope("timeline revision overflowed".to_owned())
                })?;
                stream.put(
                    KeyspaceName::Meta,
                    super::super::revision_key("timeline")?,
                    next.to_be_bytes().to_vec(),
                );
                ensure_not_cancelled(cancel)
            })?;
        Ok(())
    }
}

fn collect_counts(
    store: &TimelineRecordStore,
    snapshot: &SourceReadSnapshot,
    source_id: &str,
    cancel: &AtomicBool,
    progress: &mut impl FnMut(u64),
) -> Result<HashMap<Vec<u8>, i64>> {
    let mut deltas = HashMap::new();
    let mut cursor = None;
    let mut processed = 0u64;
    progress(processed);
    loop {
        ensure_not_cancelled(cancel)?;
        let page = store.list_by_source(
            snapshot,
            source_id,
            cursor.as_deref(),
            ScanDirection::Forward,
            PAGE_SIZE,
        )?;
        for record in page.records {
            ensure_not_cancelled(cancel)?;
            if record.data_source_id != source_id {
                return Err(FjallStorageError::TimelineRecordIdentityMismatch);
            }
            adjust_count_deltas(&mut deltas, &record, 1)?;
            processed = processed.checked_add(1).ok_or_else(|| {
                FjallStorageError::InvalidEnvelope(
                    "timeline rebuild progress overflowed".to_owned(),
                )
            })?;
        }
        progress(processed);
        cursor = page.next_cursor;
        if cursor.is_none() {
            return Ok(deltas);
        }
    }
}

fn clear_counts(
    snapshot: &SourceReadSnapshot,
    stream: &mut WriteStream<'_>,
    source_id: &str,
    cancel: &AtomicBool,
) -> Result<()> {
    let prefix = encode_key(COUNT_FAMILY, &[KeyPart::Text(source_id.to_owned())])?;
    let mut cursor = None;
    loop {
        ensure_not_cancelled(cancel)?;
        let page = snapshot.keyset(
            KeyspaceName::TimelineIndexes,
            &prefix,
            cursor.as_deref(),
            ScanDirection::Forward,
            PAGE_SIZE,
        )?;
        cursor = page.last().map(|entry| entry.key.clone());
        let count = page.len();
        for entry in page {
            ensure_not_cancelled(cancel)?;
            stream.delete(KeyspaceName::TimelineIndexes, entry.key);
        }
        if count < PAGE_SIZE {
            return Ok(());
        }
    }
}

fn stream_action(stream: &mut WriteStream<'_>, action: WriteAction) {
    match action {
        WriteAction::Put {
            keyspace,
            key,
            value,
        } => stream.put(keyspace, key, value),
        WriteAction::Delete { keyspace, key } => stream.delete(keyspace, key),
    }
}

fn ensure_not_cancelled(cancel: &AtomicBool) -> Result<()> {
    if cancel.load(Ordering::Relaxed) {
        Err(FjallStorageError::Cancelled)
    } else {
        Ok(())
    }
}
