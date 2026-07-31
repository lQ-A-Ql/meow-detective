use chrono::{DateTime, Utc};
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

fn build_histogram(
    sources: &[(domain::DataSourceId, Connection, TimelineFacetSummary)],
    time_start: Option<i64>,
    time_end: Option<i64>,
    event_type: Option<&str>,
    min_epoch: i64,
    max_epoch: i64,
    bucket_count: u32,
) -> Result<Vec<TimelineHistogramBucketDto>, TimelineServiceError> {
    let mut counts = vec![0u64; bucket_count as usize];
    for (_, connection, _) in sources {
        for (bucket, count) in TimelineFacetsRepo::new(connection).bucket_counts(
            time_start,
            time_end,
            event_type,
            min_epoch,
            max_epoch,
            bucket_count,
        )? {
            if let Some(slot) = counts.get_mut(bucket as usize) {
                *slot = slot.saturating_add(count);
            }
        }
    }
    let range = i128::from(max_epoch) - i128::from(min_epoch);
    (0..bucket_count)
        .map(|index| {
            let start =
                i128::from(min_epoch) + range * i128::from(index) / i128::from(bucket_count);
            let end = if index + 1 == bucket_count {
                i128::from(max_epoch)
            } else {
                i128::from(min_epoch) + range * i128::from(index + 1) / i128::from(bucket_count)
            };
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
