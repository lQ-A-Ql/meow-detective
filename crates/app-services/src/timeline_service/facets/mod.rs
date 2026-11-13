mod histogram;
mod native;
mod source;

use chrono::{DateTime, Utc};
use domain::DataSourceId;
use rusqlite::Connection;
use std::{collections::BTreeMap, path::Path};
use transport::dto::{TimelineFacetCountDto, TimelineFacetsDto};

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

    let mut ready_source_ids = source_db::ready_data_sources(case_conn, case_id)?
        .into_iter()
        .map(|(source, _)| source.id)
        .collect::<Vec<DataSourceId>>();
    ready_source_ids.sort_by(|left, right| left.0.cmp(&right.0));
    native::get(
        case_root,
        &ready_source_ids,
        time_start,
        time_end,
        event_type,
        bucket_count,
    )
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

/// Converts one validated epoch-second boundary into a facet window edge.
fn epoch_to_datetime(
    label: &str,
    epoch: Option<i64>,
) -> Result<Option<DateTime<Utc>>, TimelineServiceError> {
    epoch
        .map(|value| {
            DateTime::<Utc>::from_timestamp(value, 0).ok_or_else(|| {
                TimelineServiceError::InvalidInput(format!(
                    "{label} must be a representable timeline timestamp"
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
        event_types: facet_counts(event_types),
        data_sources: Vec::new(),
        histogram: Vec::new(),
    }
}

fn facet_counts(event_types: BTreeMap<String, u64>) -> Vec<TimelineFacetCountDto> {
    event_types
        .into_iter()
        .map(|(value, count)| TimelineFacetCountDto { value, count })
        .collect()
}
