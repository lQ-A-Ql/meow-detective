use super::{index_keys, record_action, record_key, validate_replacement_record};
use crate::{
    timeline::projection_state::{
        timeline_projection_marker_envelope, SegmentLimits, SegmentedReplaceOutcome,
        TimelineProjectionState, TimelineProjectionStatePayload,
    },
    FjallStorageError, KeyspaceName, PersistMode, Result, SourceReadSnapshot, TimelineRecord,
    TimelineRecordStore, TimelineStreamError, WriteAction, WriteSessionConfig,
};
use std::{
    collections::BTreeMap,
    hash::{Hash, Hasher},
    sync::atomic::AtomicBool,
};

mod spool;
use spool::RecordSpool;

impl TimelineRecordStore {
    #[allow(clippy::too_many_arguments)]
    pub fn replace_by_parser_prefix_parallel<E, F>(
        &self,
        source: &str,
        prefix: &str,
        identity: &str,
        mode: PersistMode,
        limits: SegmentLimits,
        config: WriteSessionConfig,
        cancel: &AtomicBool,
        producer: F,
    ) -> std::result::Result<SegmentedReplaceOutcome, TimelineStreamError<E>>
    where
        E: std::error::Error + 'static,
        F: FnMut() -> std::result::Result<Option<Vec<TimelineRecord>>, E>,
    {
        config.validate()?;
        if source.is_empty()
            || prefix.is_empty()
            || identity.is_empty()
            || limits.max_bytes == 0
            || limits.max_records == 0
        {
            return Err(FjallStorageError::InvalidEnvelope(
                "invalid timeline replacement request".into(),
            )
            .into());
        }
        let limits = SegmentLimits {
            max_bytes: limits.max_bytes.min(config.batch_bytes as u64 / 3),
            max_records: limits.max_records.min(4096),
        };
        let mut staged = RecordSpool::stage(self, source, prefix, &limits, producer)?;
        write_marker(self, source, identity, TimelineProjectionState::Building, 0)?;
        let session = self.source_store.database().begin_write_session(config)?;
        let deleted = delete_existing(self, source, prefix, cancel, &session)?;
        let inserted = staged.records;
        let mut reader = staged.reader().peekable();
        let mut max_bytes = 0;
        session.run(
            cancel,
            || {
                let batch = spool::next_batch(&mut reader, limits, config.batch_bytes as u64 / 3)?;
                if let Some((_, bytes)) = &batch {
                    max_bytes = max_bytes.max(*bytes);
                }
                Ok(batch.map(|(records, _)| records))
            },
            record_route,
            |handle, records| {
                let snapshot = handle.read_snapshot();
                insertion_actions(self, &snapshot, records)
            },
        )?;
        let native_writes = session.finish()?;
        self.rebuild_count_indexes_with_control(source, mode, cancel, |_| {})?;
        write_marker(
            self,
            source,
            identity,
            TimelineProjectionState::Complete,
            inserted,
        )?;
        Ok(SegmentedReplaceOutcome {
            inserted_records: inserted,
            deleted_records: deleted,
            segments: native_writes.batches,
            max_segment_bytes: max_bytes,
            native_writes,
        })
    }
}

fn delete_existing(
    store: &TimelineRecordStore,
    source: &str,
    prefix: &str,
    cancel: &AtomicBool,
    session: &crate::SourceWriteSession,
) -> Result<u64> {
    let snapshot = store.read_snapshot();
    let mut cursor = None;
    let mut complete = false;
    let mut deleted = 0;
    session.run(
        cancel,
        || {
            while !complete {
                let page = store.scan_by_source(&snapshot, source, cursor.as_deref(), 512)?;
                cursor = page.next_cursor;
                complete = cursor.is_none();
                let records = page
                    .records
                    .into_iter()
                    .filter(|record| {
                        record
                            .parser_id
                            .as_deref()
                            .is_some_and(|id| id.starts_with(prefix))
                    })
                    .collect::<Vec<_>>();
                deleted += records.len() as u64;
                if !records.is_empty() {
                    return Ok(Some(records));
                }
            }
            Ok(None)
        },
        record_route,
        |_, records| {
            let mut actions = Vec::new();
            for record in records {
                append_delete(&mut actions, &record)?;
            }
            Ok(actions)
        },
    )?;
    Ok(deleted)
}

fn insertion_actions(
    store: &TimelineRecordStore,
    snapshot: &SourceReadSnapshot,
    records: Vec<TimelineRecord>,
) -> Result<Vec<WriteAction>> {
    let records = records
        .into_iter()
        .map(|record| ((record.data_source_id.clone(), record.id.clone()), record))
        .collect::<BTreeMap<_, _>>();
    let mut actions = Vec::new();
    for ((source, id), record) in records {
        if let Some(previous) = store.get(snapshot, &source, &id)? {
            if previous == record {
                continue;
            }
            append_delete(&mut actions, &previous)?;
        }
        actions.push(record_action(&record)?);
        let key = record_key(&source, &id)?;
        for index in index_keys(&record)? {
            actions.push(WriteAction::Put {
                keyspace: KeyspaceName::TimelineIndexes,
                key: index,
                value: key.clone(),
            });
        }
    }
    Ok(actions)
}

fn append_delete(actions: &mut Vec<WriteAction>, record: &TimelineRecord) -> Result<()> {
    actions.push(WriteAction::Delete {
        keyspace: KeyspaceName::TimelineRecords,
        key: record_key(&record.data_source_id, &record.id)?,
    });
    for key in index_keys(record)? {
        actions.push(WriteAction::Delete {
            keyspace: KeyspaceName::TimelineIndexes,
            key,
        });
    }
    Ok(())
}

fn record_route(record: &TimelineRecord) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    record.data_source_id.hash(&mut hasher);
    record.id.hash(&mut hasher);
    hasher.finish()
}

fn write_marker(
    store: &TimelineRecordStore,
    source: &str,
    identity: &str,
    state: TimelineProjectionState,
    inserted: u64,
) -> Result<()> {
    let payload = TimelineProjectionStatePayload::new(state, identity, inserted, 0, 0, None);
    let (key, envelope) = timeline_projection_marker_envelope(source, &payload)?;
    store.source_store.database().write_batch(
        &[WriteAction::Put {
            keyspace: KeyspaceName::Meta,
            key,
            value: envelope.encode()?,
        }],
        PersistMode::SyncAll,
    )?;
    Ok(())
}
