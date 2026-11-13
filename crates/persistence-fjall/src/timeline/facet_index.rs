use super::{
    decode_record, record_key, timestamp_sort_value, TimelineRecordStore, TIME_FAMILY, TYPE_FAMILY,
};
use crate::{
    codec::{encode_key, KeyPart, VersionedEnvelope},
    database::SourceReadSnapshot,
    errors::{FjallStorageError, Result},
    keyspaces::KeyspaceName,
    scan::{ScanBounds, ScanDirection, SnapshotEntry},
};
use chrono::{DateTime, TimeZone, Utc};
use std::ops::Bound;

impl TimelineRecordStore {
    /// Returns the matching edge timestamps using two bounded index probes.
    /// Bounds include every timestamp within their whole second.
    pub fn facet_window_edges(
        &self,
        snapshot: &SourceReadSnapshot,
        source_id: &str,
        event_type: Option<&str>,
        start: Option<DateTime<Utc>>,
        end: Option<DateTime<Utc>>,
    ) -> Result<Option<(DateTime<Utc>, DateTime<Utc>)>> {
        let start = whole_second_start(start)?;
        let end = whole_second_end(end)?;
        if start.zip(end).is_some_and(|(start, end)| start > end) {
            return Ok(None);
        }
        let first = index_edge_timestamp(
            snapshot,
            source_id,
            event_type,
            ScanDirection::Reverse,
            start,
        )?;
        let last =
            index_edge_timestamp(snapshot, source_id, event_type, ScanDirection::Forward, end)?;
        Ok(first.zip(last).filter(|(first, last)| first <= last))
    }
}

pub(super) fn whole_second_start(bound: Option<DateTime<Utc>>) -> Result<Option<DateTime<Utc>>> {
    bound
        .map(|value| {
            Utc.timestamp_opt(value.timestamp(), 0)
                .single()
                .ok_or(FjallStorageError::InvalidTimelineTimestampEncoding)
        })
        .transpose()
}

pub(super) fn whole_second_end(bound: Option<DateTime<Utc>>) -> Result<Option<DateTime<Utc>>> {
    bound
        .map(|value| {
            Utc.timestamp_opt(value.timestamp(), 999_999_999)
                .single()
                .ok_or(FjallStorageError::InvalidTimelineTimestampEncoding)
        })
        .transpose()
}

pub(super) fn index_edge_timestamp(
    snapshot: &SourceReadSnapshot,
    source_id: &str,
    event_type: Option<&str>,
    direction: ScanDirection,
    bound: Option<DateTime<Utc>>,
) -> Result<Option<DateTime<Utc>>> {
    let (family, parts) = index_family_and_parts(source_id, event_type);
    let entry = match (direction, bound) {
        (ScanDirection::Forward, Some(end)) => first_entry(snapshot.range_keyset(
            KeyspaceName::TimelineIndexes,
            edge_bounds(family, &parts, end, ScanDirection::Forward)?,
            None,
            1,
        )?),
        (ScanDirection::Forward, None) => first_entry(snapshot.keyset(
            KeyspaceName::TimelineIndexes,
            &encode_key(family, &parts)?,
            None,
            ScanDirection::Forward,
            1,
        )?),
        (ScanDirection::Reverse, Some(start)) => first_entry(snapshot.range_keyset_reverse(
            KeyspaceName::TimelineIndexes,
            edge_bounds(family, &parts, start, ScanDirection::Reverse)?,
            None,
            1,
        )?),
        (ScanDirection::Reverse, None) => first_entry(snapshot.keyset(
            KeyspaceName::TimelineIndexes,
            &encode_key(family, &parts)?,
            None,
            ScanDirection::Reverse,
            1,
        )?),
    };
    let Some(entry) = entry else {
        return Ok(None);
    };
    timestamp_for_index_entry(snapshot, &entry.value).map(Some)
}

pub(super) fn index_family_and_parts(
    source_id: &str,
    event_type: Option<&str>,
) -> (&'static str, Vec<KeyPart>) {
    let mut parts = vec![KeyPart::Text(source_id.to_owned())];
    match event_type {
        Some(kind) => {
            parts.push(KeyPart::Text(kind.to_owned()));
            (TYPE_FAMILY, parts)
        }
        None => (TIME_FAMILY, parts),
    }
}

pub(super) fn edge_bounds(
    family: &str,
    prefix_parts: &[KeyPart],
    bound: DateTime<Utc>,
    direction: ScanDirection,
) -> Result<ScanBounds> {
    let family_prefix = encode_key(family, prefix_parts)?;
    let mut parts = prefix_parts.to_vec();
    parts.push(KeyPart::InverseU64(timestamp_sort_value(bound)));
    let bound_key = encode_key(family, &parts)?;
    match direction {
        ScanDirection::Forward => Ok(ScanBounds {
            start: Bound::Included(bound_key),
            end: Bound::Excluded(prefix_end(&family_prefix)?),
        }),
        ScanDirection::Reverse => Ok(ScanBounds {
            start: Bound::Included(family_prefix),
            end: Bound::Excluded(prefix_end(&bound_key)?),
        }),
    }
}

fn first_entry(entries: Vec<SnapshotEntry>) -> Option<SnapshotEntry> {
    entries.into_iter().next()
}

fn timestamp_for_index_entry(
    snapshot: &SourceReadSnapshot,
    index_value: &[u8],
) -> Result<DateTime<Utc>> {
    let bytes = snapshot
        .get(KeyspaceName::TimelineRecords, index_value)?
        .ok_or(FjallStorageError::TimelineIndexCorrupt)?;
    let record = decode_record(VersionedEnvelope::decode(&bytes)?)?;
    if record_key(&record.data_source_id, &record.id)? != index_value {
        return Err(FjallStorageError::TimelineRecordIdentityMismatch);
    }
    Ok(record.timestamp)
}

fn prefix_end(prefix: &[u8]) -> Result<Vec<u8>> {
    let mut end = prefix.to_vec();
    while let Some(last) = end.last_mut() {
        if *last == u8::MAX {
            end.pop();
        } else {
            *last += 1;
            return Ok(end);
        }
    }
    Err(FjallStorageError::InvalidEnvelope(
        "timeline facet prefix has no upper bound".to_owned(),
    ))
}
