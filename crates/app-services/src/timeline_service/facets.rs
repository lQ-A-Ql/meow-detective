use chrono::{DateTime, Utc};
use persistence_source::fjall::{ScanDirection, TimelineRecordStore};
use persistence_sqlite::repositories::timeline_facets_repo::{
    TimelineFacetSummary, TimelineFacetsRepo,
};
use rusqlite::Connection;
use std::{collections::BTreeMap, path::Path};
use transport::dto::{TimelineFacetCountDto, TimelineFacetsDto, TimelineHistogramBucketDto};

use super::TimelineServiceError;
use crate::source_db;

pub fn get_timeline_facets_for_case(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &domain::CaseId,
    time_start: Option<&str>,
    time_end: Option<&str>,
    event_type: Option<&str>,
    bucket_count: u32,
) -> Result<TimelineFacetsDto, TimelineServiceError> {
    let time_start = parse_boundary("timeStart", time_start)?;
    let time_end = parse_boundary("timeEnd", time_end)?;
    if let (Some(start), Some(end)) = (time_start, time_end) {
        if start > end {
            return Err(TimelineServiceError::InvalidInput(
                "timeStart must be before or equal to timeEnd".to_string(),
            ));
        }
    }

    let ready_source_ids =
        source_db::open_ready_source_connections_read_only(case_conn, case_root, case_id)?
            .into_iter()
            .map(|(id, _)| id)
            .collect::<Vec<_>>();
    let mut ready_source_ids = ready_source_ids;
    ready_source_ids.sort_by(|left, right| left.0.cmp(&right.0));
    if ready_source_ids.iter().all(|source_id| {
        source_db::open_registered_fjall_source_read_only(case_root, source_id).is_ok()
    }) {
        return get_native_timeline_facets(
            case_root,
            ready_source_ids,
            time_start,
            time_end,
            event_type,
            bucket_count,
        );
    }
    let mut sources = Vec::new();
    let mut total = 0u64;
    let mut min_epoch = None;
    let mut max_epoch = None;
    let mut event_types = BTreeMap::<String, u64>::new();

    for (source_id, connection) in
        source_db::open_ready_source_connections_read_only(case_conn, case_root, case_id)?
    {
        let repository = TimelineFacetsRepo::new(&connection);
        for (kind, count) in repository.event_type_counts(time_start, time_end, None)? {
            let entry = event_types.entry(kind).or_default();
            *entry = entry.saturating_add(count);
        }
        let summary = repository.summary(time_start, time_end, event_type)?;
        if summary.total == 0 {
            continue;
        }
        total = total.saturating_add(summary.total);
        min_epoch = match (min_epoch, summary.min_epoch) {
            (None, value) => value,
            (Some(current), Some(value)) => Some(current.min(value)),
            (Some(current), None) => Some(current),
        };
        max_epoch = max_epoch.max(summary.max_epoch);
        sources.push((source_id, connection, summary));
    }

    let Some(min_epoch) = min_epoch else {
        return Ok(empty_facets(event_types));
    };
    let max_epoch = max_epoch.unwrap_or(min_epoch);
    let histogram = build_histogram(
        &sources,
        time_start,
        time_end,
        event_type,
        min_epoch,
        max_epoch,
        bucket_count,
    )?;
    Ok(TimelineFacetsDto {
        total_events: total,
        start_ts: Some(format_epoch(min_epoch)?),
        end_ts: Some(format_epoch(max_epoch)?),
        event_types: event_types
            .into_iter()
            .map(|(value, count)| TimelineFacetCountDto { value, count })
            .collect(),
        data_sources: sources
            .into_iter()
            .map(|(source_id, _, summary)| TimelineFacetCountDto {
                value: source_id.0,
                count: summary.total,
            })
            .collect(),
        histogram,
    })
}

struct NativeFacetSource {
    id: domain::DataSourceId,
    total: u64,
    min_epoch: Option<i64>,
    max_epoch: Option<i64>,
    event_types: BTreeMap<String, u64>,
}

fn get_native_timeline_facets(
    case_root: &Path,
    source_ids: Vec<domain::DataSourceId>,
    time_start: Option<i64>,
    time_end: Option<i64>,
    event_type: Option<&str>,
    bucket_count: u32,
) -> Result<TimelineFacetsDto, TimelineServiceError> {
    let mut sources = Vec::new();
    let mut event_types = BTreeMap::new();
    for id in source_ids {
        let source = source_db::open_registered_fjall_source_read_only(case_root, &id)
            .map_err(TimelineServiceError::from)?;
        let snapshot = source.snapshot();
        source.require_projection_complete(&snapshot, &id.0)?;
        let repository = TimelineRecordStore::from_source_store(source.source_store().clone());
        let mut cursor = None;
        let mut summary = NativeFacetSource {
            id: id.clone(),
            total: 0,
            min_epoch: None,
            max_epoch: None,
            event_types: BTreeMap::new(),
        };
        loop {
            let page = repository
                .list_by_time(
                    &snapshot,
                    &id.0,
                    cursor.as_deref(),
                    ScanDirection::Forward,
                    512,
                )
                .map_err(|error| TimelineServiceError::Other(error.to_string()))?;
            for record in page.records {
                let epoch = record.timestamp.timestamp();
                *summary
                    .event_types
                    .entry(record.event_type.clone())
                    .or_default() += 1;
                if event_type.is_some_and(|kind| kind != record.event_type) {
                    continue;
                }
                if time_start.is_some_and(|start| epoch < start)
                    || time_end.is_some_and(|end| epoch > end)
                {
                    continue;
                }
                summary.total += 1;
                summary.min_epoch = Some(summary.min_epoch.map_or(epoch, |value| value.min(epoch)));
                summary.max_epoch = Some(summary.max_epoch.map_or(epoch, |value| value.max(epoch)));
            }
            cursor = page.next_cursor;
            if cursor.is_none() {
                break;
            }
        }
        for (kind, count) in &summary.event_types {
            *event_types.entry(kind.clone()).or_default() += count;
        }
        sources.push(summary);
    }
    let Some(min_epoch) = sources.iter().filter_map(|source| source.min_epoch).min() else {
        return Ok(empty_facets(event_types));
    };
    let max_epoch = sources
        .iter()
        .filter_map(|source| source.max_epoch)
        .max()
        .unwrap_or(min_epoch);
    let histogram = native_histogram(
        case_root,
        &sources,
        time_start,
        time_end,
        event_type,
        min_epoch,
        max_epoch,
        bucket_count,
    )?;
    Ok(TimelineFacetsDto {
        total_events: sources.iter().map(|source| source.total).sum(),
        start_ts: Some(format_epoch(min_epoch)?),
        end_ts: Some(format_epoch(max_epoch)?),
        event_types: event_types
            .into_iter()
            .map(|(value, count)| TimelineFacetCountDto { value, count })
            .collect(),
        data_sources: sources
            .iter()
            .map(|source| TimelineFacetCountDto {
                value: source.id.0.clone(),
                count: source.total,
            })
            .collect(),
        histogram,
    })
}

#[allow(clippy::too_many_arguments)]
fn native_histogram(
    case_root: &Path,
    sources: &[NativeFacetSource],
    time_start: Option<i64>,
    time_end: Option<i64>,
    event_type: Option<&str>,
    min_epoch: i64,
    max_epoch: i64,
    bucket_count: u32,
) -> Result<Vec<TimelineHistogramBucketDto>, TimelineServiceError> {
    let span = max_epoch.saturating_sub(min_epoch);
    let count = bucket_count.max(1).min(
        u32::try_from(span.saturating_add(1))
            .unwrap_or(u32::MAX)
            .max(1),
    );
    let mut buckets = vec![0u64; count as usize];
    for source in sources {
        let handle = source_db::open_registered_fjall_source_read_only(case_root, &source.id)
            .map_err(TimelineServiceError::from)?;
        let snapshot = handle.snapshot();
        let repository = TimelineRecordStore::from_source_store(handle.source_store().clone());
        let mut cursor = None;
        loop {
            let page = repository
                .list_by_time(
                    &snapshot,
                    &source.id.0,
                    cursor.as_deref(),
                    ScanDirection::Forward,
                    512,
                )
                .map_err(|error| TimelineServiceError::Other(error.to_string()))?;
            for record in page.records {
                let epoch = record.timestamp.timestamp();
                if event_type.is_some_and(|kind| kind != record.event_type)
                    || time_start.is_some_and(|start| epoch < start)
                    || time_end.is_some_and(|end| epoch > end)
                {
                    continue;
                }
                let index = (((epoch - min_epoch) as i128 * count as i128) / (span as i128 + 1))
                    .min((count - 1) as i128) as usize;
                buckets[index] += 1;
            }
            cursor = page.next_cursor;
            if cursor.is_none() {
                break;
            }
        }
    }
    let span = i128::from(span) + 1;
    (0..count)
        .map(|index| {
            let start = i128::from(min_epoch) + span * index as i128 / count as i128;
            let end = (i128::from(min_epoch) + span * (index + 1) as i128 / count as i128 - 1)
                .min(i128::from(max_epoch));
            Ok(TimelineHistogramBucketDto {
                start_ts: format_epoch(i64::try_from(start).map_err(|_| {
                    TimelineServiceError::Other("timeline histogram start overflow".into())
                })?)?,
                end_ts: format_epoch(i64::try_from(end).map_err(|_| {
                    TimelineServiceError::Other("timeline histogram end overflow".into())
                })?)?,
                count: buckets[index as usize],
            })
        })
        .collect()
}

fn build_histogram(
    sources: &[(domain::DataSourceId, Connection, TimelineFacetSummary)],
    time_start: Option<i64>,
    time_end: Option<i64>,
    event_type: Option<&str>,
    min_epoch: i64,
    max_epoch: i64,
    bucket_count: u32,
) -> Result<Vec<TimelineHistogramBucketDto>, TimelineServiceError> {
    let range_seconds = max_epoch.saturating_sub(min_epoch);
    let effective_bucket_count = bucket_count.max(1).min(
        u32::try_from(range_seconds.saturating_add(1))
            .unwrap_or(u32::MAX)
            .max(1),
    );
    let mut counts = vec![0u64; effective_bucket_count as usize];
    for (_, connection, _) in sources {
        for (bucket, count) in TimelineFacetsRepo::new(connection).bucket_counts(
            time_start,
            time_end,
            event_type,
            min_epoch,
            max_epoch,
            effective_bucket_count,
        )? {
            if let Some(slot) = counts.get_mut(bucket as usize) {
                *slot = slot.saturating_add(count);
            }
        }
    }
    let inclusive_span = i128::from(range_seconds) + 1;
    (0..effective_bucket_count)
        .map(|index| {
            let start = i128::from(min_epoch)
                + inclusive_span * i128::from(index) / i128::from(effective_bucket_count);
            let next_start = i128::from(min_epoch)
                + inclusive_span * i128::from(index + 1) / i128::from(effective_bucket_count);
            let end = next_start.saturating_sub(1).min(i128::from(max_epoch));
            Ok(TimelineHistogramBucketDto {
                start_ts: format_epoch(i64::try_from(start).map_err(|_| {
                    TimelineServiceError::Other("timeline histogram start overflow".to_string())
                })?)?,
                end_ts: format_epoch(i64::try_from(end).map_err(|_| {
                    TimelineServiceError::Other("timeline histogram end overflow".to_string())
                })?)?,
                count: counts[index as usize],
            })
        })
        .collect()
}

fn parse_boundary(label: &str, value: Option<&str>) -> Result<Option<i64>, TimelineServiceError> {
    value
        .map(|value| {
            DateTime::parse_from_rfc3339(value)
                .map(|timestamp| timestamp.timestamp())
                .map_err(|_| {
                    TimelineServiceError::InvalidInput(format!(
                        "{label} must be an RFC3339 timestamp"
                    ))
                })
        })
        .transpose()
}

fn format_epoch(epoch: i64) -> Result<String, TimelineServiceError> {
    DateTime::<Utc>::from_timestamp(epoch, 0)
        .map(|timestamp| timestamp.to_rfc3339())
        .ok_or_else(|| {
            TimelineServiceError::Other("timeline timestamp is out of range".to_string())
        })
}

fn empty_facets(event_types: BTreeMap<String, u64>) -> TimelineFacetsDto {
    TimelineFacetsDto {
        total_events: 0,
        start_ts: None,
        end_ts: None,
        event_types: event_types
            .into_iter()
            .map(|(value, count)| TimelineFacetCountDto { value, count })
            .collect(),
        data_sources: Vec::new(),
        histogram: Vec::new(),
    }
}
