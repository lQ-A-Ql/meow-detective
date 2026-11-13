use rusqlite::Connection;
use std::path::Path;
use transport::{
    dto::{TimelineAggregatedDto, TimelineEventDto},
    paging::{validate_page_offset, PageResponse},
};

use super::{TimelineQuery, TimelineServiceError};

mod aggregation_native;

pub(super) struct ClusterRow {
    pub event_type: String,
    pub description: String,
    pub count: u64,
    pub first_ts: String,
    pub last_ts: String,
    pub sample_ids: String,
}

pub fn query_timeline_for_case(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &domain::CaseId,
    offset: u64,
    limit: u32,
) -> Result<PageResponse<TimelineEventDto>, TimelineServiceError> {
    query_timeline_filtered_for_case(
        case_conn,
        case_root,
        case_id,
        TimelineQuery::unfiltered(offset, limit),
    )
}

pub fn query_timeline_filtered_for_case(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &domain::CaseId,
    query: TimelineQuery<'_>,
) -> Result<PageResponse<TimelineEventDto>, TimelineServiceError> {
    validate_page_offset(query.offset)
        .map_err(|error| TimelineServiceError::InvalidInput(error.to_string()))?;
    if query.cursor.is_some() && query.offset != 0 {
        return Err(TimelineServiceError::InvalidInput(
            "offset must be zero when cursor is provided".to_string(),
        ));
    }
    super::fjall::query(case_conn, case_root, case_id, query)
}

pub fn query_timeline_aggregated(
    conn: &Connection,
    offset: u64,
    limit: u32,
) -> Result<TimelineAggregatedDto, TimelineServiceError> {
    validate_page_offset(offset)
        .map_err(|error| TimelineServiceError::InvalidInput(error.to_string()))?;
    if persistence_source::detail::source_id(conn)?.is_none() {
        return Err(TimelineServiceError::Other(
            "source identity is missing".into(),
        ));
    }
    aggregation_native::query(conn, offset, limit)
}
