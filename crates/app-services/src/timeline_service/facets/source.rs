use chrono::{DateTime, Utc};
use domain::DataSourceId;
use persistence_source::fjall::{
    FacetSummary, FjallStorageError, SourceReadSnapshot, TimelineRecordStore,
};
use std::{collections::BTreeMap, path::Path};

use super::epoch_to_datetime;
use super::histogram::Histogram;
use crate::{source_db, timeline_service::TimelineServiceError};

pub(super) struct FacetFilter<'a> {
    pub start: Option<i64>,
    pub end: Option<i64>,
    pub event_type: Option<&'a str>,
}

pub(super) struct NativeFacetSource {
    pub id: DataSourceId,
    pub bounds: Option<(DateTime<Utc>, DateTime<Utc>)>,
    store: TimelineRecordStore,
    snapshot: SourceReadSnapshot,
    summary: FacetSummary,
}

impl NativeFacetSource {
    pub(super) fn open(
        case_root: &Path,
        id: &DataSourceId,
        filter: &FacetFilter<'_>,
    ) -> Result<Self, TimelineServiceError> {
        let handle = source_db::open_registered_fjall_source_read_only(case_root, id)?;
        let snapshot = handle.snapshot();
        handle.require_projection_complete(&snapshot, &id.0)?;
        let store = TimelineRecordStore::from_source_store(handle.source_store().clone());
        let start = epoch_to_datetime("timeStart", filter.start)?;
        let end = epoch_to_datetime("timeEnd", filter.end)?;
        let summary = store.facet_summary(&snapshot, &id.0, filter.event_type, start, end)?;
        let bounds = summary.min_timestamp.zip(summary.max_timestamp);
        if bounds.is_some_and(|(first, last)| first > last) {
            return Err(FjallStorageError::TimelineIndexCorrupt.into());
        }
        Ok(Self {
            id: id.clone(),
            bounds,
            store,
            snapshot,
            summary,
        })
    }

    pub(super) fn collect(
        &self,
        filter: &FacetFilter<'_>,
        histogram: &mut Histogram,
        types: &mut BTreeMap<String, u64>,
    ) -> Result<u64, TimelineServiceError> {
        for (kind, count) in &self.summary.type_totals {
            add_type_count(types, kind, *count)?;
        }
        let observed = self.collect_window(filter, histogram)?;
        if observed != self.summary.total {
            return Err(FjallStorageError::TimelineIndexCorrupt.into());
        }
        Ok(observed)
    }

    fn collect_window(
        &self,
        filter: &FacetFilter<'_>,
        histogram: &mut Histogram,
    ) -> Result<u64, TimelineServiceError> {
        let Some((first, last)) = self.bounds else {
            return Ok(0);
        };
        let windows = histogram
            .windows()
            .into_iter()
            .map(|(start, end)| {
                let start = DateTime::<Utc>::from_timestamp(start.max(first.timestamp()), 0)
                    .ok_or(FjallStorageError::InvalidTimelineTimestampEncoding)?;
                let end = DateTime::<Utc>::from_timestamp(end.min(last.timestamp()), 999_999_999)
                    .ok_or(FjallStorageError::InvalidTimelineTimestampEncoding)?;
                Ok(start..=end)
            })
            .collect::<Result<Vec<_>, FjallStorageError>>()?;
        let counts =
            self.store
                .count_windows(&self.snapshot, &self.id.0, filter.event_type, &windows)?;
        let total = counts.iter().try_fold(0u64, |sum, count| {
            sum.checked_add(*count)
                .ok_or(FjallStorageError::TimelineIndexCorrupt)
        })?;
        histogram.add_counts(&counts)?;
        Ok(total)
    }
}

fn add_type_count(
    types: &mut BTreeMap<String, u64>,
    kind: &str,
    value: u64,
) -> Result<(), TimelineServiceError> {
    let count = types.entry(kind.to_owned()).or_default();
    *count = count.checked_add(value).ok_or_else(|| {
        TimelineServiceError::Other("timeline event type count overflow".to_owned())
    })?;
    Ok(())
}
