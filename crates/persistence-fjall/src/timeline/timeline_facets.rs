//! Facet aggregation helpers for the timeline listing UI.
//!
//! Summaries require the published `timeline.count` index and use bounded
//! index probes. Empty sources return an empty summary.

use super::facet_index::{
    edge_bounds, index_edge_timestamp, index_family_and_parts, whole_second_end, whole_second_start,
};
use super::timeline_count::counts_ready;
use super::{decode_record, TimelinePage, TimelineRecordStore, RECORD_FAMILY};
use crate::{
    codec::{decode_key, encode_key, KeyPart, VersionedEnvelope},
    database::SourceReadSnapshot,
    errors::{FjallStorageError, Result},
    keyspaces::KeyspaceName,
    scan::{ScanBounds, ScanDirection},
};
use chrono::{DateTime, Utc};
use std::collections::BTreeMap;

const COUNT_FAMILY: &str = "timeline.count";
const TYPE_SCOPE: &[u8] = b"type";
const TOTAL_PERIOD: &[u8] = b"total";

/// Time-window and type aggregation for one source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FacetSummary {
    /// Records matching the event-type and window filters.
    pub total: u64,
    /// Oldest matching record timestamp, `None` when `total` is zero.
    pub min_timestamp: Option<DateTime<Utc>>,
    /// Newest matching record timestamp, `None` when `total` is zero.
    pub max_timestamp: Option<DateTime<Utc>>,
    /// Every record of the source per event type, ignoring the filters.
    pub type_totals: BTreeMap<String, u64>,
}

impl TimelineRecordStore {
    /// Aggregates one source for the timeline facet panel.
    ///
    /// `start` and `end` are inclusive epoch-second bounds: a record matches
    /// when its whole-second timestamp falls inside the window, which mirrors
    /// the long-standing epoch filter of the facet service. `None` leaves the
    /// corresponding side unbounded.
    pub fn facet_summary(
        &self,
        snapshot: &SourceReadSnapshot,
        source_id: &str,
        event_type: Option<&str>,
        start: Option<DateTime<Utc>>,
        end: Option<DateTime<Utc>>,
    ) -> Result<FacetSummary> {
        let start = whole_second_start(start)?;
        let end = whole_second_end(end)?;
        if !counts_ready(snapshot, source_id)? {
            let prefix = encode_key(RECORD_FAMILY, &[KeyPart::Text(source_id.to_owned())])?;
            let first = snapshot.keyset(
                KeyspaceName::TimelineRecords,
                &prefix,
                None,
                ScanDirection::Forward,
                1,
            )?;
            if !first.is_empty() {
                return Err(FjallStorageError::TimelineIndexCorrupt);
            }
            return Ok(FacetSummary {
                total: 0,
                min_timestamp: None,
                max_timestamp: None,
                type_totals: BTreeMap::new(),
            });
        }
        self.counted_facet_summary(snapshot, source_id, event_type, start, end)
    }

    fn counted_facet_summary(
        &self,
        snapshot: &SourceReadSnapshot,
        source_id: &str,
        event_type: Option<&str>,
        start: Option<DateTime<Utc>>,
        end: Option<DateTime<Utc>>,
    ) -> Result<FacetSummary> {
        let type_totals = count_type_totals(snapshot, source_id)?;
        let total = match event_type {
            Some(kind) => self.count_by_type(snapshot, source_id, kind, start, end)?,
            None => self.count_by_time(snapshot, source_id, start, end)?,
        };
        let (min_timestamp, max_timestamp) = if total == 0 {
            (None, None)
        } else {
            (
                Some(
                    index_edge_timestamp(
                        snapshot,
                        source_id,
                        event_type,
                        ScanDirection::Reverse,
                        start,
                    )?
                    .ok_or(FjallStorageError::TimelineIndexCorrupt)?,
                ),
                Some(
                    index_edge_timestamp(
                        snapshot,
                        source_id,
                        event_type,
                        ScanDirection::Forward,
                        end,
                    )?
                    .ok_or(FjallStorageError::TimelineIndexCorrupt)?,
                ),
            )
        };
        Ok(FacetSummary {
            total,
            min_timestamp,
            max_timestamp,
            type_totals,
        })
    }

    pub fn list_in_time_window(
        &self,
        snapshot: &SourceReadSnapshot,
        source_id: &str,
        event_type: Option<&str>,
        window: std::ops::RangeInclusive<DateTime<Utc>>,
        cursor: Option<&[u8]>,
        limit: usize,
    ) -> Result<TimelinePage> {
        if limit == 0 || window.is_empty() {
            return Ok(TimelinePage {
                records: Vec::new(),
                next_cursor: None,
            });
        }
        let (family, parts) = index_family_and_parts(source_id, event_type);
        let bounds = ScanBounds {
            start: edge_bounds(family, &parts, *window.end(), ScanDirection::Forward)?.start,
            end: edge_bounds(family, &parts, *window.start(), ScanDirection::Reverse)?.end,
        };
        let entries = snapshot.range_keyset(
            KeyspaceName::TimelineIndexes,
            bounds,
            cursor,
            limit.saturating_add(1),
        )?;
        let has_more = entries.len() > limit;
        let entries = entries.into_iter().take(limit).collect::<Vec<_>>();
        let mut records = Vec::with_capacity(entries.len());
        for entry in &entries {
            let bytes = snapshot
                .get(KeyspaceName::TimelineRecords, &entry.value)?
                .ok_or(FjallStorageError::TimelineIndexCorrupt)?;
            records.push(decode_record(VersionedEnvelope::decode(&bytes)?)?);
        }
        let next_cursor = has_more
            .then(|| entries.last().map(|entry| entry.key.clone()))
            .flatten();
        Ok(TimelinePage {
            records,
            next_cursor,
        })
    }
}

/// Reads the per-type record totals from one `timeline.count` prefix scan.
fn count_type_totals(
    snapshot: &SourceReadSnapshot,
    source_id: &str,
) -> Result<BTreeMap<String, u64>> {
    let prefix = encode_key(COUNT_FAMILY, &[KeyPart::Text(source_id.to_owned())])?;
    let mut totals = BTreeMap::new();
    let mut cursor = None;
    loop {
        let page = snapshot.keyset(
            KeyspaceName::TimelineIndexes,
            &prefix,
            cursor.as_deref(),
            ScanDirection::Forward,
            512,
        )?;
        for entry in &page {
            let (family, parts) = decode_key(&entry.key)?;
            if family != COUNT_FAMILY {
                return Err(FjallStorageError::TimelineIndexCorrupt);
            }
            let (scope, kind, period) = counter_parts(&parts)?;
            if scope != TYPE_SCOPE || period != TOTAL_PERIOD {
                continue;
            }
            let kind = std::str::from_utf8(kind)
                .map_err(|_| FjallStorageError::TimelineIndexCorrupt)?
                .to_owned();
            totals.insert(kind, counter_value(&entry.value)?);
        }
        if page.len() < 512 {
            break;
        }
        cursor = page.last().map(|entry| entry.key.clone());
    }
    Ok(totals)
}

fn counter_parts(parts: &[KeyPart]) -> Result<(&[u8], &[u8], &[u8])> {
    match parts {
        [KeyPart::Bytes(_source), KeyPart::Bytes(scope), KeyPart::Bytes(kind), KeyPart::Bytes(period)] => {
            Ok((scope, kind, period))
        }
        _ => Err(FjallStorageError::TimelineIndexCorrupt),
    }
}

fn counter_value(bytes: &[u8]) -> Result<u64> {
    let encoded: [u8; 8] = bytes
        .try_into()
        .map_err(|_| FjallStorageError::TimelineIndexCorrupt)?;
    Ok(u64::from_be_bytes(encoded))
}
