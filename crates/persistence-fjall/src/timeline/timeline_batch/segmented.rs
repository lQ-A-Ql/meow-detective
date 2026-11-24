//! Segmented parser-prefix replacement: fixed read snapshot for prefix
//! enumeration, one write transaction per segment, `building` marker before
//! the first delete commit, and `complete` attached to the final data
//! segment. The producer is drained before any destructive delete so a
//! producer error leaves the previous projection untouched.

use super::{
    index_keys, record_action, record_key, stream_action, stream_delete,
    validate_replacement_record, TimelineRecord, TimelineRecordStore,
};
use crate::{
    codec::VersionedEnvelope,
    database::{PersistMode, SourceReadSnapshot, WriteAction, WriteStream},
    errors::{FjallStorageError, Result},
    keyspaces::KeyspaceName,
    timeline::projection_state::{
        timeline_projection_marker_envelope, SegmentLimits, SegmentedReplaceOutcome,
        TimelineProjectionState, TimelineProjectionStatePayload,
    },
    TimelineStreamError,
};
use std::collections::{BTreeMap, HashMap};
mod spool;
use spool::RecordSpool;

const DELETE_SEGMENT_MAX_RECORDS: usize = 512;

struct InsertPhaseSummary {
    inserted: u64,
    segments: u64,
    max_segment_bytes: u64,
}

struct CommittedSegment {
    count: usize,
    bytes: u64,
}

pub(super) fn segmented_replace_exec<E, F>(
    store: &TimelineRecordStore,
    source_id: &str,
    prefix: &str,
    identity: &str,
    mode: PersistMode,
    limits: SegmentLimits,
    next_batch: F,
) -> std::result::Result<SegmentedReplaceOutcome, TimelineStreamError<E>>
where
    E: std::error::Error + 'static,
    F: FnMut() -> std::result::Result<Option<Vec<TimelineRecord>>, E>,
{
    validate_segmented_request(source_id, prefix, identity)
        .map_err(TimelineStreamError::Storage)?;
    if limits.max_bytes == 0 || limits.max_records == 0 {
        return Err(TimelineStreamError::Storage(
            FjallStorageError::InvalidEnvelope("timeline segment limits must be positive".into()),
        ));
    }
    let mut staged = RecordSpool::stage(store, source_id, prefix, &limits, next_batch)?;
    write_building_marker(store, source_id, identity).map_err(TimelineStreamError::Storage)?;

    let snapshot = store.source_store().database().read_snapshot();
    let (deleted_records, delete_segments) =
        delete_prefix_segmented(store, &snapshot, source_id, prefix, mode)
            .map_err(TimelineStreamError::Storage)?;
    let summary = insert_staged(store, source_id, identity, mode, limits, &mut staged)
        .map_err(TimelineStreamError::Storage)?;

    Ok(SegmentedReplaceOutcome {
        inserted_records: summary.inserted,
        deleted_records,
        segments: delete_segments + summary.segments,
        max_segment_bytes: summary.max_segment_bytes,
    })
}

fn validate_segmented_request(source_id: &str, prefix: &str, identity: &str) -> Result<()> {
    if prefix.is_empty() {
        return Err(FjallStorageError::InvalidEnvelope(
            "timeline replacement prefix is empty".into(),
        ));
    }
    if source_id.is_empty() || identity.is_empty() {
        return Err(FjallStorageError::InvalidEnvelope(
            "timeline projection identity fields are empty".into(),
        ));
    }
    Ok(())
}

fn write_building_marker(
    store: &TimelineRecordStore,
    source_id: &str,
    identity: &str,
) -> Result<()> {
    let payload = TimelineProjectionStatePayload::new(
        TimelineProjectionState::Building,
        identity.to_owned(),
        0,
        0,
        0,
        None,
    );
    write_marker(store, source_id, &payload)
}

fn write_complete_marker(
    store: &TimelineRecordStore,
    source_id: &str,
    identity: &str,
    inserted_events: u64,
) -> Result<()> {
    let payload = TimelineProjectionStatePayload::new(
        TimelineProjectionState::Complete,
        identity.to_owned(),
        inserted_events,
        0,
        0,
        None,
    );
    write_marker(store, source_id, &payload)
}

fn write_marker(
    store: &TimelineRecordStore,
    source_id: &str,
    payload: &TimelineProjectionStatePayload,
) -> Result<()> {
    let (key, envelope) = timeline_projection_marker_envelope(source_id, payload)?;
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

fn delete_prefix_segmented(
    store: &TimelineRecordStore,
    snapshot: &SourceReadSnapshot,
    source_id: &str,
    prefix: &str,
    mode: PersistMode,
) -> Result<(u64, u64)> {
    let mut deleted = 0u64;
    let mut segments = 0u64;
    let mut cursor: Option<Vec<u8>> = None;
    loop {
        let page = store.scan_by_source(
            snapshot,
            source_id,
            cursor.as_deref(),
            DELETE_SEGMENT_MAX_RECORDS,
        )?;
        let matching: Vec<TimelineRecord> = page
            .records
            .into_iter()
            .filter(|record| {
                record
                    .parser_id
                    .as_deref()
                    .is_some_and(|id| id.starts_with(prefix))
            })
            .collect();
        cursor = page.next_cursor;
        if !matching.is_empty() {
            let count = matching.len();
            commit_delete_segment(store, matching, mode)?;
            deleted = deleted.saturating_add(count as u64);
            segments += 1;
        }
        if cursor.is_none() {
            return Ok((deleted, segments));
        }
    }
}

fn commit_delete_segment(
    store: &TimelineRecordStore,
    records: Vec<TimelineRecord>,
    mode: PersistMode,
) -> Result<()> {
    let snapshot = store.source_store.database().read_snapshot();
    store
        .source_store
        .database()
        .write_streamed_with_snapshot(mode, &snapshot, |snap, writer| {
            let mut count_deltas = HashMap::new();
            let mut count_policies = BTreeMap::new();
            for record in &records {
                let policy = count_policies
                    .entry(record.data_source_id.clone())
                    .or_insert(crate::timeline::timeline_count::count_write_policy(
                        snap,
                        &record.data_source_id,
                    )?);
                if policy.0 {
                    crate::timeline::timeline_count::adjust_count_deltas(
                        &mut count_deltas,
                        record,
                        -1,
                    )?;
                }
                stream_delete(writer, record)?;
            }
            store.finish_streamed_mutations(snap, writer, count_deltas, count_policies)?;
            Ok::<(), FjallStorageError>(())
        })
        .map(|_| ())
}

fn insert_staged(
    store: &TimelineRecordStore,
    source_id: &str,
    identity: &str,
    mode: PersistMode,
    limits: SegmentLimits,
    staged: &mut RecordSpool,
) -> Result<InsertPhaseSummary> {
    if staged.records == 0 {
        write_complete_marker(store, source_id, identity, 0)?;
        return Ok(InsertPhaseSummary {
            inserted: 0,
            segments: 0,
            max_segment_bytes: 0,
        });
    }

    let total = staged.records;
    let mut inserted = 0u64;
    let mut segments = 0u64;
    let mut max_segment_bytes = 0u64;
    let mut buffered: Vec<TimelineRecord> = Vec::new();
    let mut buffered_bytes = 0u64;

    for staged_record in staged.reader() {
        let spool::StagedRecord {
            record,
            write_bytes,
        } = staged_record?;
        if !buffered.is_empty()
            && segment_full(buffered.len(), buffered_bytes, write_bytes, &limits)
        {
            let committed = commit_insert_segment(
                store,
                std::mem::take(&mut buffered),
                buffered_bytes,
                false,
                source_id,
                identity,
                inserted,
                mode,
            )?;
            segments += 1;
            inserted += committed.count as u64;
            max_segment_bytes = max_segment_bytes.max(committed.bytes);
            buffered_bytes = 0;
        }
        buffered_bytes = buffered_bytes.saturating_add(write_bytes);
        buffered.push(record);
    }

    let committed = commit_insert_segment(
        store,
        buffered,
        buffered_bytes,
        true,
        source_id,
        identity,
        total,
        mode,
    )?;
    segments += 1;
    inserted += committed.count as u64;
    max_segment_bytes = max_segment_bytes.max(committed.bytes);

    Ok(InsertPhaseSummary {
        inserted,
        segments,
        max_segment_bytes,
    })
}

fn segment_full(
    buffered_len: usize,
    buffered_bytes: u64,
    next_bytes: u64,
    limits: &SegmentLimits,
) -> bool {
    buffered_len >= limits.max_records
        || buffered_bytes.saturating_add(next_bytes) > limits.max_bytes
}

#[allow(clippy::too_many_arguments)]
fn commit_insert_segment(
    store: &TimelineRecordStore,
    records: Vec<TimelineRecord>,
    segment_bytes: u64,
    with_complete: bool,
    source_id: &str,
    identity: &str,
    inserted_total: u64,
    mode: PersistMode,
) -> Result<CommittedSegment> {
    let count = records.len();
    let complete_payload = with_complete.then(|| {
        TimelineProjectionStatePayload::new(
            TimelineProjectionState::Complete,
            identity.to_owned(),
            inserted_total,
            0,
            0,
            None,
        )
    });
    let snapshot = store.source_store.database().read_snapshot();
    store.source_store.database().write_streamed_with_snapshot(
        mode,
        &snapshot,
        |snap, writer| {
            let mut count_deltas = HashMap::new();
            let mut count_policies = BTreeMap::new();
            for record in &records {
                apply_insert_record(snap, writer, record, &mut count_deltas, &mut count_policies)?;
            }
            store.finish_streamed_mutations(snap, writer, count_deltas, count_policies)?;
            if let Some(payload) = &complete_payload {
                let (key, envelope) = timeline_projection_marker_envelope(source_id, payload)?;
                writer.put(KeyspaceName::Meta, key, envelope.encode()?);
            }
            Ok::<(), FjallStorageError>(())
        },
    )?;
    Ok(CommittedSegment {
        count,
        bytes: segment_bytes,
    })
}

fn apply_insert_record(
    snapshot: &SourceReadSnapshot,
    writer: &mut WriteStream<'_>,
    record: &TimelineRecord,
    count_deltas: &mut HashMap<Vec<u8>, i64>,
    count_policies: &mut BTreeMap<String, (bool, bool)>,
) -> Result<()> {
    let policy = count_policies
        .entry(record.data_source_id.clone())
        .or_insert(crate::timeline::timeline_count::count_write_policy(
            snapshot,
            &record.data_source_id,
        )?);
    let existing = writer
        .get(
            KeyspaceName::TimelineRecords,
            &record_key(&record.data_source_id, &record.id)?,
        )?
        .map(|bytes| {
            VersionedEnvelope::decode(&bytes).and_then(|envelope| {
                if envelope.record_id != record.id || envelope.source_id != record.data_source_id {
                    return Err(FjallStorageError::TimelineRecordIdentityMismatch);
                }
                crate::timeline::decode_record(envelope)
            })
        })
        .transpose()?;
    if existing.as_ref().is_some_and(|existing| existing == record) {
        return Ok(());
    }
    if let Some(previous) = existing.as_ref() {
        if policy.0 {
            crate::timeline::timeline_count::adjust_count_deltas(count_deltas, previous, -1)?;
        }
        stream_delete(writer, previous)?;
    }
    stream_action(writer, record_action(record)?);
    for index in index_keys(record)? {
        writer.put(
            KeyspaceName::TimelineIndexes,
            index,
            record_key(&record.data_source_id, &record.id)?,
        );
    }
    if policy.0 {
        crate::timeline::timeline_count::adjust_count_deltas(count_deltas, record, 1)?;
    }
    Ok(())
}

impl TimelineRecordStore {
    pub fn replace_by_parser_prefix_segmented<E, F>(
        &self,
        source_id: &str,
        prefix: &str,
        identity: &str,
        mode: PersistMode,
        limits: SegmentLimits,
        next_batch: F,
    ) -> std::result::Result<SegmentedReplaceOutcome, TimelineStreamError<E>>
    where
        E: std::error::Error + 'static,
        F: FnMut() -> std::result::Result<Option<Vec<TimelineRecord>>, E>,
    {
        segmented_replace_exec(self, source_id, prefix, identity, mode, limits, next_batch)
    }
}
