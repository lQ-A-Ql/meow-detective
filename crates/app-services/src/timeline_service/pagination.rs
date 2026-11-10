use persistence_sqlite::repositories::timeline_repo::{
    TimelineCursorRow, TimelineRepo, TimelineSortKey,
};
use rusqlite::Connection;
use std::collections::VecDeque;
use std::path::Path;
use transport::{
    dto::{TimelineAggregatedDto, TimelineEventDto},
    paging::PageResponse,
};

use super::export::timeline_event_to_source_dto;
use super::{TimelineQuery, TimelineServiceError};
use crate::source_db;

mod aggregation_native;
mod cursor;

const TIMELINE_MERGE_BATCH_SIZE: u32 = 256;

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
    if query.cursor.is_some() && query.offset != 0 {
        return Err(TimelineServiceError::InvalidInput(
            "offset must be zero when cursor is provided".to_string(),
        ));
    }
    if query.cursor.is_some() || query.offset == 0 {
        if let Some(page) = super::fjall::try_query(case_conn, case_root, case_id, query)? {
            return Ok(page);
        }
        return cursor::query_cursor_page(case_conn, case_root, case_id, query);
    }
    if let Some(page) = super::fjall::try_query(case_conn, case_root, case_id, query)? {
        return Ok(page);
    }
    query_timeline_offset_page(case_conn, case_root, case_id, query)
}

fn query_timeline_offset_page(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &domain::CaseId,
    query: TimelineQuery<'_>,
) -> Result<PageResponse<TimelineEventDto>, TimelineServiceError> {
    let mut total = 0u64;
    let mut sources = Vec::new();

    for (source_id, source_conn) in
        source_db::open_ready_source_connections_read_only(case_conn, case_root, case_id)?
    {
        let repo = TimelineRepo::new(&source_conn);
        total = total.saturating_add(repo.count_filtered(
            query.time_start,
            query.time_end,
            query.event_type,
        )?);
        sources.push(SourceTimelineCursor::new(source_id, source_conn));
    }

    if query.limit == 0 || query.offset >= total {
        return Ok(PageResponse {
            total,
            items: Vec::new(),
            next_cursor: None,
        });
    }

    let scan_end = query
        .offset
        .saturating_add(u64::from(query.limit))
        .min(total);
    let batch_size = query.limit.max(TIMELINE_MERGE_BATCH_SIZE);
    let mut position = 0u64;
    let mut items = Vec::with_capacity(query.limit as usize);
    while position < scan_end {
        for source in &mut sources {
            source.refill(query, batch_size)?;
        }
        let Some(source_index) = next_timeline_source(&sources) else {
            break;
        };
        let source = &mut sources[source_index];
        let event = source.buffer.pop_front().ok_or_else(|| {
            TimelineServiceError::Other(
                "timeline merge cursor selected a source without a buffered event".to_string(),
            )
        })?;
        if position >= query.offset {
            items.push(timeline_event_to_source_dto(event.event, &source.source_id));
        }
        position = position.saturating_add(1);
    }
    Ok(PageResponse {
        total,
        items,
        next_cursor: None,
    })
}

struct SourceTimelineCursor {
    source_id: domain::DataSourceId,
    connection: Connection,
    after: Option<TimelineSortKey>,
    buffer: VecDeque<TimelineCursorRow>,
    exhausted: bool,
}

impl SourceTimelineCursor {
    fn new(source_id: domain::DataSourceId, connection: Connection) -> Self {
        Self {
            source_id,
            connection,
            after: None,
            buffer: VecDeque::new(),
            exhausted: false,
        }
    }

    fn refill(
        &mut self,
        query: TimelineQuery<'_>,
        batch_size: u32,
    ) -> Result<(), TimelineServiceError> {
        if self.exhausted || !self.buffer.is_empty() {
            return Ok(());
        }
        let rows = TimelineRepo::new(&self.connection).query_filtered_after(
            self.after.as_ref(),
            batch_size,
            query.time_start,
            query.time_end,
            query.event_type,
        )?;
        let fetched = rows.len();
        if let Some(last) = rows.last() {
            self.after = Some(last.sort_key.clone());
        }
        self.buffer.extend(rows);
        self.exhausted = fetched < batch_size as usize;
        Ok(())
    }
}

fn next_timeline_source(sources: &[SourceTimelineCursor]) -> Option<usize> {
    let mut selected: Option<(usize, &SourceTimelineCursor, &TimelineCursorRow)> = None;
    for (index, source) in sources.iter().enumerate() {
        let Some(candidate) = source.buffer.front() else {
            continue;
        };
        if selected
            .as_ref()
            .is_none_or(|(_, current_source, current)| {
                timeline_precedes(source, candidate, current_source, current)
            })
        {
            selected = Some((index, source, candidate));
        }
    }
    selected.map(|(index, _, _)| index)
}

fn timeline_precedes(
    left_source: &SourceTimelineCursor,
    left: &TimelineCursorRow,
    right_source: &SourceTimelineCursor,
    right: &TimelineCursorRow,
) -> bool {
    left.sort_key.timestamp > right.sort_key.timestamp
        || (left.sort_key.timestamp == right.sort_key.timestamp
            && (left_source.source_id.0 < right_source.source_id.0
                || (left_source.source_id == right_source.source_id
                    && left.sort_key.event_id < right.sort_key.event_id)))
}

pub fn query_timeline_aggregated(
    conn: &Connection,
    offset: u64,
    limit: u32,
) -> Result<TimelineAggregatedDto, TimelineServiceError> {
    if persistence_sqlite::source_fjall::source_id(conn)?.is_none() {
        return Err(TimelineServiceError::Other(
            "source identity is missing".into(),
        ));
    }
    query_native_timeline_aggregated(conn, offset, limit)
}

fn query_native_timeline_aggregated(
    conn: &Connection,
    offset: u64,
    limit: u32,
) -> Result<TimelineAggregatedDto, TimelineServiceError> {
    aggregation_native::query(conn, offset, limit)
}
