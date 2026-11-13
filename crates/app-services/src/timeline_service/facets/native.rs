use domain::DataSourceId;
use std::{collections::BTreeMap, path::Path};
use transport::dto::{TimelineFacetCountDto, TimelineFacetsDto};

use super::histogram::Histogram;
use super::source::{FacetFilter, NativeFacetSource};
use super::{empty_facets, facet_counts, format_epoch};
use crate::timeline_service::TimelineServiceError;

pub(super) fn get(
    case_root: &Path,
    source_ids: &[DataSourceId],
    time_start: Option<i64>,
    time_end: Option<i64>,
    event_type: Option<&str>,
    bucket_count: u32,
) -> Result<TimelineFacetsDto, TimelineServiceError> {
    let filter = FacetFilter {
        start: time_start,
        end: time_end,
        event_type,
    };
    let sources = source_ids
        .iter()
        .map(|id| NativeFacetSource::open(case_root, id, &filter))
        .collect::<Result<Vec<_>, _>>()?;
    let min_epoch = sources
        .iter()
        .filter_map(|source| source.bounds.map(|(first, _)| first.timestamp()))
        .min();
    let max_epoch = sources
        .iter()
        .filter_map(|source| source.bounds.map(|(_, last)| last.timestamp()))
        .max();
    let mut histogram = Histogram::new(min_epoch.zip(max_epoch), bucket_count);
    let mut event_types = BTreeMap::new();
    let mut data_sources = Vec::with_capacity(sources.len());
    let mut total_events = 0u64;
    for source in &sources {
        let count = source.collect(&filter, &mut histogram, &mut event_types)?;
        total_events = total_events.checked_add(count).ok_or_else(|| {
            TimelineServiceError::Other("timeline facet total overflow".to_owned())
        })?;
        data_sources.push(TimelineFacetCountDto {
            value: source.id.0.clone(),
            count,
        });
    }
    let Some((min_epoch, max_epoch)) = min_epoch.zip(max_epoch) else {
        return Ok(empty_facets(event_types));
    };
    Ok(TimelineFacetsDto {
        total_events,
        start_ts: Some(format_epoch(min_epoch)?),
        end_ts: Some(format_epoch(max_epoch)?),
        event_types: facet_counts(event_types),
        data_sources,
        histogram: histogram.finish()?,
    })
}
