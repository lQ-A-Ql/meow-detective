use rusqlite::Connection;
use std::path::Path;
use transport::{
    dto::{PerformanceReportDto, TimelineEventDto},
    paging::PageResponse,
};

use super::pagination::{query_timeline_filtered_for_case, query_timeline_for_case};
use super::query::{query_timeline, query_timeline_filtered};
use super::{TimelineQuery, TimelineServiceError};
use crate::performance::{measure_rows, metric, report, PerfSample};
use crate::source_db::encode_source_scoped_id;

#[derive(Debug, Clone)]
pub struct InstrumentedPage<T> {
    pub page: PageResponse<T>,
    pub performance_report: PerformanceReportDto,
}

pub fn query_timeline_for_case_instrumented(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &domain::CaseId,
    offset: u64,
    limit: u32,
) -> Result<InstrumentedPage<TimelineEventDto>, TimelineServiceError> {
    instrument(|| query_timeline_for_case(case_conn, case_root, case_id, offset, limit))
}

pub fn query_timeline_instrumented(
    conn: &Connection,
    offset: u64,
    limit: u32,
) -> Result<InstrumentedPage<TimelineEventDto>, TimelineServiceError> {
    instrument(|| query_timeline(conn, offset, limit))
}

pub fn query_timeline_filtered_instrumented(
    conn: &Connection,
    offset: u64,
    limit: u32,
    time_start: Option<&str>,
    time_end: Option<&str>,
    event_type: Option<&str>,
) -> Result<InstrumentedPage<TimelineEventDto>, TimelineServiceError> {
    instrument(|| query_timeline_filtered(conn, offset, limit, time_start, time_end, event_type))
}

pub fn query_timeline_filtered_for_case_instrumented(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &domain::CaseId,
    query: TimelineQuery<'_>,
) -> Result<InstrumentedPage<TimelineEventDto>, TimelineServiceError> {
    instrument(|| query_timeline_filtered_for_case(case_conn, case_root, case_id, query))
}

fn instrument<F>(operation: F) -> Result<InstrumentedPage<TimelineEventDto>, TimelineServiceError>
where
    F: FnOnce() -> Result<PageResponse<TimelineEventDto>, TimelineServiceError>,
{
    let (page, sample) = measure_rows(0, operation);
    let page = page?;
    let sample = PerfSample {
        rows: page.items.len() as u64,
        ..sample
    };
    Ok(InstrumentedPage {
        performance_report: timeline_query_report(sample, page.total),
        page,
    })
}

fn timeline_query_report(sample: PerfSample, total: u64) -> PerformanceReportDto {
    let prefix = "timeline.query";
    let mut metrics = vec![
        metric(
            format!("{prefix}.elapsedMs"),
            sample.elapsed_ms as f64,
            "ms",
        ),
        metric(format!("{prefix}.rows"), sample.rows as f64, "rows"),
        metric(format!("{prefix}.totalRows"), total as f64, "rows"),
    ];
    if let Some(rows_per_sec) = sample.rows_per_sec() {
        metrics.push(metric(
            format!("{prefix}.rowsPerSec"),
            rows_per_sec,
            "rows/s",
        ));
    }
    report(
        format!("{prefix}:{}:{}", sample.elapsed_ms, sample.rows),
        None,
        sample.elapsed_ms,
        format!(
            "Timeline query returned {} rows in {} ms",
            sample.rows, sample.elapsed_ms
        ),
        metrics,
    )
}

pub(super) fn timeline_event_to_dto(event: domain::TimelineEvent) -> TimelineEventDto {
    TimelineEventDto {
        id: event.id.0,
        source_object_id: event.source_object_id,
        event_type: event.event_type,
        ts: event.timestamp.to_rfc3339(),
        title: event.title,
        description: event.description,
        parser_id: event.parser_id,
        parser_version: event.parser_version,
        confidence: event.confidence,
        source_attribution: event.source_attribution,
        attrs: event.attrs,
    }
}

pub(super) fn timeline_event_to_source_dto(
    event: domain::TimelineEvent,
    data_source_id: &domain::DataSourceId,
) -> TimelineEventDto {
    let mut dto = timeline_event_to_dto(event);
    dto.id = encode_source_scoped_id(data_source_id, &dto.id);
    if !dto.source_object_id.is_empty() {
        dto.source_object_id = encode_source_scoped_id(data_source_id, &dto.source_object_id);
    }
    dto
}
