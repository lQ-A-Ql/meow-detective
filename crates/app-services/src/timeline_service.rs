use transport::{dto::TimelineEventDto, paging::PageResponse};

use domain::FileEntry;
use persistence_sqlite::repositories::timeline_repo::TimelineRepo;
use rusqlite::Connection;

pub fn project_and_store_macb(conn: &Connection, files: &[FileEntry]) -> Result<u64, String> {
    let repo = TimelineRepo::new(conn);
    let mut all_events = Vec::new();
    for file in files {
        let events = timeline::project_file_macb(file);
        all_events.extend(events);
    }
    let count = all_events.len() as u64;
    if !all_events.is_empty() {
        repo.insert_batch(&all_events).map_err(|e| e.to_string())?;
    }
    Ok(count)
}

/// Query timeline events without filtering.
pub fn query_timeline(
    conn: &Connection,
    offset: u64,
    limit: u32,
) -> Result<PageResponse<TimelineEventDto>, String> {
    let repo = TimelineRepo::new(conn);
    let total = repo.count().map_err(|e| e.to_string())?;
    let events = repo.query(offset, limit).map_err(|e| e.to_string())?;
    let items: Vec<TimelineEventDto> = events
        .into_iter()
        .map(|ev| TimelineEventDto {
            id: ev.id.0,
            source_object_id: ev.source_object_id,
            event_type: ev.event_type,
            ts: ev.timestamp.to_rfc3339(),
            title: ev.title,
            description: ev.description,
            attrs: ev.attrs,
        })
        .collect();
    Ok(PageResponse { total, items })
}

/// Query timeline events with optional filtering by time range and event type.
pub fn query_timeline_filtered(
    conn: &Connection,
    offset: u64,
    limit: u32,
    time_start: Option<&str>,
    time_end: Option<&str>,
    event_type: Option<&str>,
) -> Result<PageResponse<TimelineEventDto>, String> {
    let repo = TimelineRepo::new(conn);
    let total = repo
        .count_filtered(time_start, time_end, event_type)
        .map_err(|e| e.to_string())?;
    let events = repo
        .query_filtered(offset, limit, time_start, time_end, event_type)
        .map_err(|e| e.to_string())?;
    let items: Vec<TimelineEventDto> = events
        .into_iter()
        .map(|ev| TimelineEventDto {
            id: ev.id.0,
            source_object_id: ev.source_object_id,
            event_type: ev.event_type,
            ts: ev.timestamp.to_rfc3339(),
            title: ev.title,
            description: ev.description,
            attrs: ev.attrs,
        })
        .collect();
    Ok(PageResponse { total, items })
}
