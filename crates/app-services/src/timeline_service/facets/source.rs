use chrono::{DateTime, Utc};
use domain::DataSourceId;
use persistence_source::fjall::{
    FacetSummary, FjallStorageError, ScanDirection, SourceReadSnapshot, TimelineRecord,
    TimelineRecordStore,
};
use std::{collections::BTreeMap, path::Path};

use super::epoch_to_datetime;
use super::histogram::Histogram;
use crate::{source_db, timeline_service::TimelineServiceError};

const PAGE_SIZE: usize = 512;

pub(super) struct FacetFilter<'a> {
    pub start: Option<i64>,
    pub end: Option<i64>,
    pub event_type: Option<&'a str>,
}

impl FacetFilter<'_> {
    fn matches(&self, record: &TimelineRecord) -> bool {
        self.event_type.is_none_or(|kind| record.event_type == kind)
            && self
                .start
                .is_none_or(|bound| record.timestamp.timestamp() >= bound)
            && self
                .end
                .is_none_or(|bound| record.timestamp.timestamp() <= bound)
    }
}

pub(super) struct NativeFacetSource {
    pub id: DataSourceId,
    pub bounds: Option<(DateTime<Utc>, DateTime<Utc>)>,
    store: TimelineRecordStore,
    snapshot: SourceReadSnapshot,
    indexed_summary: Option<FacetSummary>,
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
        let indexed_summary = if store.counts_ready(&snapshot, &id.0)? {
            Some(store.facet_summary(&snapshot, &id.0, filter.event_type, start, end)?)
        } else {
            None
        };
        let bounds = match &indexed_summary {
            Some(summary) => summary.min_timestamp.zip(summary.max_timestamp),
            None => store.facet_window_edges(&snapshot, &id.0, filter.event_type, start, end)?,
        };
        Ok(Self {
            id: id.clone(),
            bounds,
            store,
            snapshot,
            indexed_summary,
        })
    }

    pub(super) fn collect(
        &self,
        filter: &FacetFilter<'_>,
        histogram: &mut Histogram,
        types: &mut BTreeMap<String, u64>,
    ) -> Result<u64, TimelineServiceError> {
        if let Some(summary) = &self.indexed_summary {
            for (kind, count) in &summary.type_totals {
                add_type_count(types, kind, *count)?;
            }
            let observed = self.collect_indexed(filter, histogram)?;
            if observed != summary.total {
                return Err(FjallStorageError::TimelineIndexCorrupt.into());
            }
            return Ok(observed);
        }
        self.collect_unindexed(filter, histogram, types)
    }

    fn collect_indexed(
        &self,
        filter: &FacetFilter<'_>,
        histogram: &mut Histogram,
    ) -> Result<u64, TimelineServiceError> {
        let Some((first, last)) = self.bounds else {
            return Ok(0);
        };
        let mut cursor = None;
        let mut count = 0;
        loop {
            let page = self.store.list_in_time_window(
                &self.snapshot,
                &self.id.0,
                filter.event_type,
                first..=last,
                cursor.as_deref(),
                PAGE_SIZE,
            )?;
            for record in page.records {
                histogram.add(record.timestamp.timestamp())?;
                count += 1;
            }
            cursor = page.next_cursor;
            if cursor.is_none() {
                return Ok(count);
            }
        }
    }

    fn collect_unindexed(
        &self,
        filter: &FacetFilter<'_>,
        histogram: &mut Histogram,
        types: &mut BTreeMap<String, u64>,
    ) -> Result<u64, TimelineServiceError> {
        let mut cursor = None;
        let mut count = 0;
        loop {
            let page = self.store.list_by_time(
                &self.snapshot,
                &self.id.0,
                cursor.as_deref(),
                ScanDirection::Forward,
                PAGE_SIZE,
            )?;
            for record in page.records {
                add_type_count(types, &record.event_type, 1)?;
                if filter.matches(&record) {
                    histogram.add(record.timestamp.timestamp())?;
                    count += 1;
                }
            }
            cursor = page.next_cursor;
            if cursor.is_none() {
                return Ok(count);
            }
        }
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
