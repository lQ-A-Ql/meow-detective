use crate::connection::DbResult;
use domain::{TimelineEvent, TimelineEventId};
use rusqlite::{params, Connection};

pub struct TimelineRepo<'a> {
    conn: &'a Connection,
}

impl<'a> TimelineRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn insert_batch(&self, events: &[TimelineEvent]) -> DbResult<()> {
        self.insert_batch_with_case(events, "")
    }

    /// 插入时间线事件（带 case_id）
    pub fn insert_batch_with_case(&self, events: &[TimelineEvent], case_id: &str) -> DbResult<()> {
        let tx = self.conn.unchecked_transaction()?;
        {
            let mut stmt = tx.prepare_cached(
                "INSERT INTO timeline_events (id, case_id, source_object_id, event_type, ts, title, description, attrs)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            )?;
            for ev in events {
                stmt.execute(params![
                    ev.id.0,
                    case_id,
                    ev.source_object_id,
                    ev.event_type,
                    ev.timestamp.to_rfc3339(),
                    ev.title,
                    ev.description,
                    serde_json::to_string(&ev.attrs).unwrap_or_default(),
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn query(&self, offset: u64, limit: u32) -> DbResult<Vec<TimelineEvent>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, source_object_id, event_type, ts, title, description, attrs
             FROM timeline_events ORDER BY ts DESC LIMIT ?1 OFFSET ?2",
        )?;
        let rows = stmt.query_map(params![limit, offset], |row| {
            let attrs_str: String = row.get(6)?;
            Ok(TimelineEvent {
                id: TimelineEventId(row.get(0)?),
                source_object_id: row.get(1)?,
                event_type: row.get(2)?,
                timestamp: crate::util::parse_datetime(&row.get::<_, String>(3)?),
                title: row.get(4)?,
                description: row.get(5)?,
                attrs: serde_json::from_str(&attrs_str).unwrap_or_default(),
            })
        })?;
        let mut events = Vec::new();
        for row in rows {
            events.push(row?);
        }
        Ok(events)
    }

    pub fn count(&self) -> DbResult<u64> {
        let n: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM timeline_events", [], |r| r.get(0))?;
        Ok(n as u64)
    }

    /// Query timeline events with optional filtering.
    pub fn query_filtered(
        &self,
        offset: u64,
        limit: u32,
        time_start: Option<&str>,
        time_end: Option<&str>,
        event_type: Option<&str>,
    ) -> DbResult<Vec<TimelineEvent>> {
        let mut sql = String::from(
            "SELECT id, source_object_id, event_type, ts, title, description, attrs
             FROM timeline_events WHERE 1=1",
        );
        let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        let mut param_index = 1;

        if let Some(start) = time_start {
            sql.push_str(&format!(" AND ts >= ?{}", param_index));
            param_values.push(Box::new(start.to_string()));
            param_index += 1;
        }
        if let Some(end) = time_end {
            sql.push_str(&format!(" AND ts <= ?{}", param_index));
            param_values.push(Box::new(end.to_string()));
            param_index += 1;
        }
        if let Some(et) = event_type {
            sql.push_str(&format!(" AND event_type = ?{}", param_index));
            param_values.push(Box::new(et.to_string()));
            param_index += 1;
        }

        sql.push_str(&format!(
            " ORDER BY ts DESC LIMIT ?{} OFFSET ?{}",
            param_index,
            param_index + 1
        ));
        param_values.push(Box::new(limit));
        param_values.push(Box::new(offset));

        let mut stmt = self.conn.prepare(&sql)?;
        let params_refs: Vec<&dyn rusqlite::types::ToSql> =
            param_values.iter().map(|p| p.as_ref()).collect();
        let rows = stmt.query_map(params_refs.as_slice(), |row| {
            let attrs_str: String = row.get(6)?;
            Ok(TimelineEvent {
                id: TimelineEventId(row.get(0)?),
                source_object_id: row.get(1)?,
                event_type: row.get(2)?,
                timestamp: crate::util::parse_datetime(&row.get::<_, String>(3)?),
                title: row.get(4)?,
                description: row.get(5)?,
                attrs: serde_json::from_str(&attrs_str).unwrap_or_default(),
            })
        })?;
        let mut events = Vec::new();
        for row in rows {
            events.push(row?);
        }
        Ok(events)
    }

    /// Count timeline events with optional filtering.
    pub fn count_filtered(
        &self,
        time_start: Option<&str>,
        time_end: Option<&str>,
        event_type: Option<&str>,
    ) -> DbResult<u64> {
        let mut sql = String::from("SELECT COUNT(*) FROM timeline_events WHERE 1=1");
        let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        let mut param_index = 1;

        if let Some(start) = time_start {
            sql.push_str(&format!(" AND ts >= ?{}", param_index));
            param_values.push(Box::new(start.to_string()));
            param_index += 1;
        }
        if let Some(end) = time_end {
            sql.push_str(&format!(" AND ts <= ?{}", param_index));
            param_values.push(Box::new(end.to_string()));
            param_index += 1;
        }
        if let Some(et) = event_type {
            sql.push_str(&format!(" AND event_type = ?{}", param_index));
            param_values.push(Box::new(et.to_string()));
        }

        let mut stmt = self.conn.prepare(&sql)?;
        let params_refs: Vec<&dyn rusqlite::types::ToSql> =
            param_values.iter().map(|p| p.as_ref()).collect();
        let n: i64 = stmt.query_row(params_refs.as_slice(), |r| r.get(0))?;
        Ok(n as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn setup_db() -> rusqlite::Connection {
        let conn = crate::connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE timeline_events (
                id TEXT PRIMARY KEY NOT NULL,
                case_id TEXT NOT NULL DEFAULT '',
                source_object_id TEXT NOT NULL,
                event_type TEXT NOT NULL,
                ts TEXT NOT NULL,
                title TEXT NOT NULL,
                description TEXT NOT NULL DEFAULT '',
                attrs TEXT NOT NULL DEFAULT '{}'
            );",
        )
        .unwrap();
        conn
    }

    fn make_event(id: &str, event_type: &str, ts: &str) -> TimelineEvent {
        TimelineEvent {
            id: TimelineEventId(id.to_string()),
            source_object_id: "src-1".to_string(),
            event_type: event_type.to_string(),
            timestamp: chrono::DateTime::parse_from_rfc3339(ts)
                .unwrap()
                .with_timezone(&chrono::Utc),
            title: format!("Event {}", id),
            description: format!("Desc {}", id),
            attrs: BTreeMap::new(),
        }
    }

    #[test]
    fn insert_batch_then_count_returns_correct_number() {
        let conn = setup_db();
        let repo = TimelineRepo::new(&conn);
        let events = vec![
            make_event("e1", "file_create", "2025-01-01T00:00:00Z"),
            make_event("e2", "file_modify", "2025-01-01T01:00:00Z"),
            make_event("e3", "file_create", "2025-01-01T02:00:00Z"),
        ];
        repo.insert_batch(&events).unwrap();

        assert_eq!(repo.count().unwrap(), 3);
    }

    #[test]
    fn query_returns_events_ordered_by_timestamp() {
        let conn = setup_db();
        let repo = TimelineRepo::new(&conn);
        let events = vec![
            make_event("e1", "file_create", "2025-01-01T00:00:00Z"),
            make_event("e2", "file_modify", "2025-01-02T00:00:00Z"),
            make_event("e3", "file_delete", "2025-01-03T00:00:00Z"),
        ];
        repo.insert_batch(&events).unwrap();

        let results = repo.query(0, 10).unwrap();
        assert_eq!(results.len(), 3);
        // ORDER BY ts DESC
        assert_eq!(results[0].id.0, "e3");
        assert_eq!(results[1].id.0, "e2");
        assert_eq!(results[2].id.0, "e1");
    }

    #[test]
    fn query_filtered_filters_by_event_type() {
        let conn = setup_db();
        let repo = TimelineRepo::new(&conn);
        let events = vec![
            make_event("e1", "file_create", "2025-01-01T00:00:00Z"),
            make_event("e2", "file_modify", "2025-01-02T00:00:00Z"),
            make_event("e3", "file_create", "2025-01-03T00:00:00Z"),
        ];
        repo.insert_batch(&events).unwrap();

        let results = repo
            .query_filtered(0, 10, None, None, Some("file_create"))
            .unwrap();
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|e| e.event_type == "file_create"));
    }

    #[test]
    fn count_filtered_with_time_range() {
        let conn = setup_db();
        let repo = TimelineRepo::new(&conn);
        let events = vec![
            make_event("e1", "file_create", "2025-01-01T00:00:00Z"),
            make_event("e2", "file_modify", "2025-01-02T00:00:00Z"),
            make_event("e3", "file_delete", "2025-01-03T00:00:00Z"),
        ];
        repo.insert_batch(&events).unwrap();

        // Count events on Jan 2 only (use +00:00 format to match to_rfc3339())
        let count = repo
            .count_filtered(
                Some("2025-01-02T00:00:00+00:00"),
                Some("2025-01-02T23:59:59+00:00"),
                None,
            )
            .unwrap();
        assert_eq!(count, 1);

        // Count all
        let count_all = repo.count_filtered(None, None, None).unwrap();
        assert_eq!(count_all, 3);
    }
}
