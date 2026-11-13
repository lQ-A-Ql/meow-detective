//! Facet aggregation helpers for the timeline listing UI.
//!
//! When the `timeline.count` counters are ready a summary costs a handful of
//! point reads and bounded index probes; otherwise a single newest-first scan
//! reproduces the same numbers. Both paths answer the same question so the
//! caller can merge sources without knowing which strategy was used.

use super::facet_index::{
    edge_bounds, index_edge_timestamp, index_family_and_parts, whole_second_end, whole_second_start,
};
use super::timeline_count::counts_ready;
use super::{decode_record, TimelinePage, TimelineRecordStore};
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
const FACET_PAGE_SIZE: usize = 512;

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
    /// Whether the summary came from the `timeline.count` counters.
    pub counts_ready: bool,
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
        if counts_ready(snapshot, source_id)? {
            self.counted_facet_summary(snapshot, source_id, event_type, start, end)
        } else {
            self.scanned_facet_summary(snapshot, source_id, event_type, start, end)
        }
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
            counts_ready: true,
        })
    }

    fn scanned_facet_summary(
        &self,
        snapshot: &SourceReadSnapshot,
        source_id: &str,
        event_type: Option<&str>,
        start: Option<DateTime<Utc>>,
        end: Option<DateTime<Utc>>,
    ) -> Result<FacetSummary> {
        let mut type_totals = BTreeMap::new();
        let mut total = 0u64;
        let mut min_timestamp: Option<DateTime<Utc>> = None;
        let mut max_timestamp: Option<DateTime<Utc>> = None;
        let mut cursor = None;
        loop {
            let page = self.list_by_time(
                snapshot,
                source_id,
                cursor.as_deref(),
                ScanDirection::Forward,
                FACET_PAGE_SIZE,
            )?;
            for record in page.records {
                *type_totals.entry(record.event_type.clone()).or_default() += 1;
                if !matches_filters(&record.event_type, record.timestamp, event_type, start, end) {
                    continue;
                }
                total += 1;
                min_timestamp = Some(
                    min_timestamp.map_or(record.timestamp, |value| value.min(record.timestamp)),
                );
                max_timestamp = Some(
                    max_timestamp.map_or(record.timestamp, |value| value.max(record.timestamp)),
                );
            }
            cursor = page.next_cursor;
            if cursor.is_none() {
                break;
            }
        }
        Ok(FacetSummary {
            total,
            min_timestamp,
            max_timestamp,
            type_totals,
            counts_ready: false,
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

fn matches_filters(
    record_type: &str,
    timestamp: DateTime<Utc>,
    event_type: Option<&str>,
    start: Option<DateTime<Utc>>,
    end: Option<DateTime<Utc>>,
) -> bool {
    event_type.is_none_or(|kind| kind == record_type)
        && start.is_none_or(|bound| timestamp >= bound)
        && end.is_none_or(|bound| timestamp <= bound)
}

/// Reads the per-type record totals from one `timeline.count` prefix scan.
fn count_type_totals(
    snapshot: &SourceReadSnapshot,
    source_id: &str,
) -> Result<BTreeMap<String, u64>> {
    let prefix = encode_key(COUNT_FAMILY, &[KeyPart::Text(source_id.to_owned())])?;
    let mut totals = BTreeMap::new();
    for entry in snapshot.prefix(
        KeyspaceName::TimelineIndexes,
        &prefix,
        ScanDirection::Forward,
    )? {
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
