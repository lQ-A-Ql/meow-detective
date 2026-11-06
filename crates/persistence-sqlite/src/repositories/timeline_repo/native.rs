use crate::{DbError, DbResult};
use domain::{TimelineEvent, TimelineEventId};
use rusqlite::Connection;

impl super::TimelineRepo<'_> {
    pub fn count(&self) -> DbResult<u64> {
        if let Some(store) = crate::source_fjall::open_store(self.conn)? {
            let owner = crate::source_fjall::source_id(self.conn)?
                .ok_or_else(|| DbError::System("source identity is missing".into()))?;
            let repository =
                persistence_fjall::TimelineRecordStore::from_source_store((*store).clone());
            return repository
                .count_by_time(&repository.read_snapshot(), &owner, None, None)
                .map_err(|error| DbError::System(error.to_string()));
        }
        self.conn
            .query_row("SELECT COUNT(*) FROM timeline_events", [], |row| row.get(0))
            .map_err(Into::into)
    }
}

pub(super) fn events(conn: &Connection) -> DbResult<Option<Vec<TimelineEvent>>> {
    let Some(source_id) = crate::source_fjall::source_id(conn)? else {
        return Ok(None);
    };
    let Some(store) = crate::source_fjall::open_store(conn)? else {
        return Ok(None);
    };
    let repository = persistence_fjall::TimelineRecordStore::from_source_store((*store).clone());
    let snapshot = repository.read_snapshot();
    let mut cursor = None;
    let mut events = Vec::new();
    loop {
        crate::read_operation::ensure_active()?;
        let page = repository
            .list_by_source(
                &snapshot,
                &source_id,
                cursor.as_deref(),
                persistence_fjall::ScanDirection::Forward,
                512,
            )
            .map_err(|error| DbError::System(error.to_string()))?;
        events.extend(page.records.into_iter().map(from_record));
        cursor = page.next_cursor;
        if cursor.is_none() {
            return Ok(Some(events));
        }
    }
}

fn from_record(record: persistence_fjall::TimelineRecord) -> TimelineEvent {
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
