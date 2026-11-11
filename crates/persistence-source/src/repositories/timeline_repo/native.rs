use chrono::{DateTime, Utc};
use domain::{TimelineEvent, TimelineEventId};
use persistence_fjall::{ScanDirection, TimelinePage, TimelineRecordStore, TimelineTypeAfterQuery};
use persistence_sqlite::{DbError, DbResult};
use rusqlite::Connection;

impl super::TimelineRepo<'_> {
    pub fn count(&self) -> DbResult<u64> {
        if let Some(count) = count_filtered(self.conn, None, None, None)? {
            return Ok(count);
        }
        Err(persistence_sqlite::DbError::System(
            "source identity is missing".into(),
        ))
    }
}

pub(super) fn filtered(
    conn: &Connection,
    offset: u64,
    limit: u32,
    time_start: Option<&str>,
    time_end: Option<&str>,
    event_type: Option<&str>,
) -> DbResult<Option<Vec<TimelineEvent>>> {
    let Some((repository, source_id, snapshot)) = native_store(conn)? else {
        return Ok(None);
    };
    let start = parse_boundary(time_start)?;
    let end = parse_boundary(time_end)?;
    if limit == 0 || start.zip(end).is_some_and(|(start, end)| start > end) {
        return Ok(Some(Vec::new()));
    }
    let mut cursor = None;
    let mut skipped = 0u64;
    let mut events = Vec::with_capacity(limit as usize);
    loop {
        persistence_sqlite::read_operation::ensure_active()?;
        let page = timeline_page(
            &repository,
            &snapshot,
            &source_id,
            event_type,
            cursor.as_deref(),
            512,
        )?;
        for record in page.records {
            if !timestamp_matches(record.timestamp, start, end) {
                continue;
            }
            if skipped < offset {
                skipped += 1;
                continue;
            }
            events.push(from_record(record));
            if events.len() == limit as usize {
                return Ok(Some(events));
            }
        }
        cursor = page.next_cursor;
        if cursor.is_none() {
            return Ok(Some(events));
        }
    }
}

pub(super) fn visit_by_source_object(
    conn: &Connection,
    source_object_id: &str,
    mut visit: impl FnMut(TimelineEvent) -> DbResult<()>,
) -> DbResult<Option<()>> {
    let Some((repository, source_id, snapshot)) = native_store(conn)? else {
        return Ok(None);
    };
    let mut cursor = None;
    loop {
        persistence_sqlite::read_operation::ensure_active()?;
        let page = repository
            .list_by_source_object(
                &snapshot,
                &source_id,
                source_object_id,
                cursor.as_deref(),
                ScanDirection::Forward,
                512,
            )
            .map_err(|error| DbError::System(error.to_string()))?;
        for record in page.records {
            visit(from_record(record))?;
        }
        cursor = page.next_cursor;
        if cursor.is_none() {
            return Ok(Some(()));
        }
    }
}

pub(super) fn matching(
    conn: &Connection,
    mut predicate: impl FnMut(&TimelineEvent) -> bool,
) -> DbResult<Option<Vec<TimelineEvent>>> {
    let Some((repository, source_id, snapshot)) = native_store(conn)? else {
        return Ok(None);
    };
    let mut cursor = None;
    let mut events = Vec::new();
    loop {
        persistence_sqlite::read_operation::ensure_active()?;
        let page = repository
            .list_by_time(
                &snapshot,
                &source_id,
                cursor.as_deref(),
                ScanDirection::Forward,
                512,
            )
            .map_err(|error| DbError::System(error.to_string()))?;
        for record in page.records {
            let event = from_record(record);
            if predicate(&event) {
                events.push(event);
            }
        }
        cursor = page.next_cursor;
        if cursor.is_none() {
            return Ok(Some(events));
        }
    }
}

pub(super) fn filtered_after(
    conn: &Connection,
    after: Option<&super::TimelineSortKey>,
    limit: u32,
    time_start: Option<&str>,
    time_end: Option<&str>,
    event_type: Option<&str>,
) -> DbResult<Option<Vec<super::TimelineCursorRow>>> {
    let Some((repository, source_id, snapshot)) = native_store(conn)? else {
        return Ok(None);
    };
    let start = parse_boundary(time_start)?;
    let end = parse_boundary(time_end)?;
    if limit == 0 || start.zip(end).is_some_and(|(start, end)| start > end) {
        return Ok(Some(Vec::new()));
    }
    let after_timestamp = match after {
        Some(key) => {
            let timestamp = key.timestamp.as_deref().ok_or_else(|| {
                DbError::System("native timeline cursor timestamp is missing".into())
            })?;
            Some(parse_boundary(Some(timestamp))?.ok_or_else(|| {
                DbError::System("native timeline cursor timestamp is missing".into())
            })?)
        }
        None => None,
    };
    let mut rows = Vec::with_capacity(limit as usize);
    let mut page = match (after_timestamp, after) {
        (Some(timestamp), Some(key)) => timeline_page_after(
            &repository,
            &snapshot,
            &source_id,
            event_type,
            timestamp,
            &key.event_id,
        )?,
        (None, None) => timeline_page(&repository, &snapshot, &source_id, event_type, None, 512)?,
        (None, Some(_)) => {
            return Err(DbError::System(
                "native timeline cursor generation is unsupported".into(),
            ))
        }
        (Some(_), None) => {
            return Err(DbError::System(
                "native timeline cursor event id is missing".into(),
            ))
        }
    };
    loop {
        persistence_sqlite::read_operation::ensure_active()?;
        for record in page.records {
            if !timestamp_matches(record.timestamp, start, end) {
                continue;
            }
            let event = from_record(record);
            let sort_key = super::TimelineSortKey {
                timestamp: Some(event.timestamp.to_rfc3339()),
                event_id: event.id.0.clone(),
            };
            rows.push(super::TimelineCursorRow { event, sort_key });
            if rows.len() == limit as usize {
                return Ok(Some(rows));
            }
        }
        let Some(cursor) = page.next_cursor else {
            return Ok(Some(rows));
        };
        page = timeline_page(
            &repository,
            &snapshot,
            &source_id,
            event_type,
            Some(&cursor),
            512,
        )?;
    }
}

pub(super) fn get(conn: &Connection, event_id: &str) -> DbResult<Option<Option<TimelineEvent>>> {
    let Some((repository, source_id, snapshot)) = native_store(conn)? else {
        return Ok(None);
    };
    let event = repository
        .get(&snapshot, &source_id, event_id)
        .map_err(|error| DbError::System(error.to_string()))?
        .map(from_record);
    Ok(Some(event))
}

pub(super) fn count_filtered(
    conn: &Connection,
    time_start: Option<&str>,
    time_end: Option<&str>,
    event_type: Option<&str>,
) -> DbResult<Option<u64>> {
    let Some((repository, source_id, snapshot)) = native_store(conn)? else {
        return Ok(None);
    };
    let start = parse_boundary(time_start)?;
    let end = parse_boundary(time_end)?;
    let count = match event_type {
        Some(event_type) => repository.count_by_type(&snapshot, &source_id, event_type, start, end),
        None => repository.count_by_time(&snapshot, &source_id, start, end),
    }
    .map_err(|error| DbError::System(error.to_string()))?;
    Ok(Some(count))
}

fn native_store(
    conn: &Connection,
) -> DbResult<
    Option<(
        TimelineRecordStore,
        String,
        persistence_fjall::SourceReadSnapshot,
    )>,
> {
    let Some(source_id) = crate::detail::source_id(conn)? else {
        return Ok(None);
    };
    let store = crate::detail::require_source_store(conn)?;
    let repository = TimelineRecordStore::from_source_store((*store).clone());
    let snapshot = repository.read_snapshot();
    Ok(Some((repository, source_id, snapshot)))
}

fn timeline_page(
    repository: &TimelineRecordStore,
    snapshot: &persistence_fjall::SourceReadSnapshot,
    source_id: &str,
    event_type: Option<&str>,
    cursor: Option<&[u8]>,
    limit: usize,
) -> DbResult<TimelinePage> {
    let result = match event_type {
        Some(event_type) => repository.list_by_type(
            snapshot,
            source_id,
            event_type,
            cursor,
            ScanDirection::Forward,
            limit,
        ),
        None => repository.list_by_time(snapshot, source_id, cursor, ScanDirection::Forward, limit),
    };
    result.map_err(|error| DbError::System(error.to_string()))
}

fn timeline_page_after(
    repository: &TimelineRecordStore,
    snapshot: &persistence_fjall::SourceReadSnapshot,
    source_id: &str,
    event_type: Option<&str>,
    timestamp: DateTime<Utc>,
    event_id: &str,
) -> DbResult<TimelinePage> {
    let result = match event_type {
        Some(event_type) => repository.list_by_type_after(
            snapshot,
            TimelineTypeAfterQuery {
                source_id,
                event_type,
                after_timestamp: timestamp,
                after_id: event_id,
            },
            ScanDirection::Forward,
            512,
        ),
        None => repository.list_by_time_after(
            snapshot,
            source_id,
            timestamp,
            event_id,
            ScanDirection::Forward,
            512,
        ),
    };
    result.map_err(|error| DbError::System(error.to_string()))
}

fn parse_boundary(value: Option<&str>) -> DbResult<Option<DateTime<Utc>>> {
    value
        .map(|value| {
            DateTime::parse_from_rfc3339(value)
                .map(|value| value.with_timezone(&Utc))
                .map_err(|error| DbError::System(format!("invalid timeline boundary: {error}")))
        })
        .transpose()
}

fn timestamp_matches(
    timestamp: DateTime<Utc>,
    start: Option<DateTime<Utc>>,
    end: Option<DateTime<Utc>>,
) -> bool {
    start.is_none_or(|start| timestamp >= start) && end.is_none_or(|end| timestamp <= end)
}

pub(super) fn from_record(record: persistence_fjall::TimelineRecord) -> TimelineEvent {
    TimelineEvent {
        id: TimelineEventId(record.id),
        source_object_id: record.source_object_id,
        event_type: record.event_type,
        timestamp: record.timestamp,
        title: record.title,
        description: record.description,
        parser_id: record.parser_id,
        parser_version: record.parser_version,
        confidence: record.confidence,
        source_attribution: record.source_attribution,
        attrs: record.attrs,
    }
}
