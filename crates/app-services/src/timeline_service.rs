use transport::{dto::TimelineEventDto, paging::PageResponse};

use domain::FileEntry;
use persistence_sqlite::repositories::timeline_repo::TimelineRepo;
use rayon::prelude::*;
use rusqlite::Connection;

pub fn project_and_store_macb(conn: &Connection, files: &[FileEntry]) -> Result<u64, String> {
    let repo = TimelineRepo::new(conn);

    // Parallel: generate events from all files concurrently
    let all_events: Vec<domain::TimelineEvent> = files
        .par_iter()
        .flat_map_iter(timeline::project_file_macb)
        .collect();

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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use domain::{DataSourceId, EntryType, FileEntry, FileEntryId};

    const TIMELINE_SCHEMA: &str =
        include_str!("../../persistence-sqlite/src/migrations/scripts/0005_timeline_events.sql");

    fn in_memory_db_with_timeline() -> rusqlite::Connection {
        let conn = persistence_sqlite::connection::open_in_memory().unwrap();
        conn.execute_batch(TIMELINE_SCHEMA).unwrap();
        conn
    }

    fn make_file(name: &str, path: &str, created: bool, modified: bool) -> FileEntry {
        FileEntry {
            id: FileEntryId(uuid::Uuid::new_v4().to_string()),
            parent_id: None,
            data_source_id: DataSourceId("ds-1".to_string()),
            path: path.to_string(),
            name: name.to_string(),
            entry_type: EntryType::File,
            size: Some(1024),
            ext: Some("txt".to_string()),
            deleted: false,
            created_at: if created {
                Some(Utc.with_ymd_and_hms(2024, 6, 15, 10, 0, 0).unwrap())
            } else {
                None
            },
            modified_at: if modified {
                Some(Utc.with_ymd_and_hms(2024, 6, 15, 12, 0, 0).unwrap())
            } else {
                None
            },
            accessed_at: Some(Utc.with_ymd_and_hms(2024, 6, 15, 14, 0, 0).unwrap()),
            changed_at: None,
            hash_sha256: None,
        }
    }

    #[test]
    fn project_and_store_macb_inserts_events() {
        let conn = in_memory_db_with_timeline();

        let files = vec![
            make_file("a.txt", "/a.txt", true, true),
            make_file("b.txt", "/b.txt", true, false),
        ];

        let count = project_and_store_macb(&conn, &files).unwrap();
        // a.txt: created + modified + accessed = 3 events
        // b.txt: created + accessed = 2 events
        assert_eq!(count, 5);

        let repo = persistence_sqlite::repositories::timeline_repo::TimelineRepo::new(&conn);
        let total = repo.count().unwrap();
        assert_eq!(total, 5);
    }

    #[test]
    fn project_and_store_macb_empty_files() {
        let conn = in_memory_db_with_timeline();
        let count = project_and_store_macb(&conn, &[]).unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn query_timeline_returns_inserted_events() {
        let conn = in_memory_db_with_timeline();
        let files = vec![make_file("test.txt", "/test.txt", true, true)];
        project_and_store_macb(&conn, &files).unwrap();

        let page = query_timeline(&conn, 0, 100).unwrap();
        assert_eq!(page.items.len(), 3);
        assert_eq!(page.total, 3);
    }
}
