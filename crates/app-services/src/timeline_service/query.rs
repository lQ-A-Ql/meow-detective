use persistence_sqlite::repositories::timeline_repo::TimelineRepo;
use rusqlite::Connection;
use std::path::Path;
use transport::{dto::TimelineEventDto, paging::PageResponse};

use super::export::{timeline_event_to_dto, timeline_event_to_source_dto};
use super::projection::ensure_macb_timeline_projected;
use super::TimelineServiceError;
use crate::source_db;

#[derive(Debug, Clone, Copy, Default)]
pub struct TimelineQuery<'a> {
    pub offset: u64,
    pub limit: u32,
    pub time_start: Option<&'a str>,
    pub time_end: Option<&'a str>,
    pub event_type: Option<&'a str>,
}

impl TimelineQuery<'_> {
    pub const fn unfiltered(offset: u64, limit: u32) -> Self {
        Self {
            offset,
            limit,
            time_start: None,
            time_end: None,
            event_type: None,
        }
    }
}

pub fn query_timeline(
    conn: &Connection,
    offset: u64,
    limit: u32,
) -> Result<PageResponse<TimelineEventDto>, TimelineServiceError> {
    query_timeline_filtered(conn, offset, limit, None, None, None)
}

pub fn query_timeline_filtered(
    conn: &Connection,
    offset: u64,
    limit: u32,
    time_start: Option<&str>,
    time_end: Option<&str>,
    event_type: Option<&str>,
) -> Result<PageResponse<TimelineEventDto>, TimelineServiceError> {
    ensure_macb_timeline_projected(conn)?;
    let repo = TimelineRepo::new(conn);
    let total = repo.count_filtered(time_start, time_end, event_type)?;
    let items = repo
        .query_filtered(offset, limit, time_start, time_end, event_type)?
        .into_iter()
        .map(timeline_event_to_dto)
        .collect();
    Ok(PageResponse { total, items })
}

pub fn get_timeline_event_by_id(
    conn: &Connection,
    event_id: &str,
) -> Result<Option<TimelineEventDto>, TimelineServiceError> {
    ensure_macb_timeline_projected(conn)?;
    Ok(TimelineRepo::new(conn)
        .find_by_id(event_id)?
        .map(timeline_event_to_dto))
}

pub fn get_timeline_event_by_id_for_case(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &domain::CaseId,
    event_id: &str,
) -> Result<Option<TimelineEventDto>, TimelineServiceError> {
    let (data_source_id, local_id) =
        source_db::parse_source_scoped_id("Timeline event id", event_id).map_err(|error| {
            TimelineServiceError::InvalidInput(format!(
                "{error}; source database timeline events require ds:<dataSourceId>:<localId>"
            ))
        })?;
    let source =
        source_db::open_ready_source_by_id(case_conn, case_root, case_id, &data_source_id)?;
    ensure_macb_timeline_projected(&source.connection)?;
    Ok(TimelineRepo::new(&source.connection)
        .find_by_id(&local_id)?
        .map(|event| timeline_event_to_source_dto(event, &data_source_id)))
}
