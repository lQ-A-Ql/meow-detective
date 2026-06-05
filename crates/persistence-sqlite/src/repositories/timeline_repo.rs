use crate::connection::DbResult;
use domain::{TimelineEvent, TimelineEventId};
use rusqlite::{params, Connection};

const TIMELINE_SELECT_COLUMNS: &str = "id, source_object_id, event_type, ts, title, description, parser_id, parser_version, confidence, source_attribution, attrs";
const TIMELINE_ORDER_BY: &str = "ORDER BY ts DESC, id ASC";

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
                "INSERT INTO timeline_events (id, case_id, source_object_id, event_type, ts, title, description, parser_id, parser_version, confidence, source_attribution, attrs)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
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
                    ev.parser_id,
                    ev.parser_version,
                    ev.confidence,
                    ev.source_attribution,
                    serde_json::to_string(&ev.attrs).unwrap_or_default(),
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn query(&self, offset: u64, limit: u32) -> DbResult<Vec<TimelineEvent>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {TIMELINE_SELECT_COLUMNS} FROM timeline_events {TIMELINE_ORDER_BY} LIMIT ?1 OFFSET ?2"
        ))?;
        let rows = stmt.query_map(params![limit, offset], row_to_timeline_event)?;
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
        self.query_filtered_for_case(offset, limit, None, time_start, time_end, event_type)
    }

    pub fn query_filtered_for_case(
        &self,
        offset: u64,
        limit: u32,
        case_id: Option<&str>,
        time_start: Option<&str>,
        time_end: Option<&str>,
        event_type: Option<&str>,
    ) -> DbResult<Vec<TimelineEvent>> {
        let mut sql = format!("SELECT {TIMELINE_SELECT_COLUMNS} FROM timeline_events WHERE 1=1");
        let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        let mut param_index = 1;

        if let Some(case_id) = case_id {
            sql.push_str(&format!(" AND case_id = ?{}", param_index));
            param_values.push(Box::new(case_id.to_string()));
            param_index += 1;
        }
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
            " {TIMELINE_ORDER_BY} LIMIT ?{} OFFSET ?{}",
            param_index,
            param_index + 1
        ));
        param_values.push(Box::new(limit));
        param_values.push(Box::new(offset));

        let mut stmt = self.conn.prepare(&sql)?;
        let params_refs: Vec<&dyn rusqlite::types::ToSql> =
            param_values.iter().map(|p| p.as_ref()).collect();
        let rows = stmt.query_map(params_refs.as_slice(), row_to_timeline_event)?;
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
        self.count_filtered_for_case(None, time_start, time_end, event_type)
    }

    pub fn count_filtered_for_case(
        &self,
        case_id: Option<&str>,
        time_start: Option<&str>,
        time_end: Option<&str>,
        event_type: Option<&str>,
    ) -> DbResult<u64> {
        let mut sql = String::from("SELECT COUNT(*) FROM timeline_events WHERE 1=1");
        let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        let mut param_index = 1;

        if let Some(case_id) = case_id {
            sql.push_str(&format!(" AND case_id = ?{}", param_index));
            param_values.push(Box::new(case_id.to_string()));
            param_index += 1;
        }
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

fn row_to_timeline_event(row: &rusqlite::Row<'_>) -> rusqlite::Result<TimelineEvent> {
    let attrs_str: String = row.get(10)?;
    let timestamp = row
        .get::<_, Option<String>>(3)?
        .map(|value| crate::util::parse_datetime(&value))
        .unwrap_or_default();
    Ok(TimelineEvent {
        id: TimelineEventId(row.get(0)?),
        source_object_id: row.get(1)?,
        event_type: row.get(2)?,
        timestamp,
        title: row.get(4)?,
        description: row.get(5)?,
        parser_id: row.get(6)?,
        parser_version: row.get(7)?,
        confidence: row.get(8)?,
        source_attribution: row.get(9)?,
        attrs: serde_json::from_str(&attrs_str).unwrap_or_default(),
    })
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
                parser_id TEXT,
                parser_version TEXT,
                confidence REAL,
                source_attribution TEXT,
                attrs TEXT NOT NULL DEFAULT '{}'
            );",
        )
        .unwrap();
        conn
    }

    fn setup_legacy_db() -> rusqlite::Connection {
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
            );
            ALTER TABLE timeline_events ADD COLUMN parser_id TEXT;
            ALTER TABLE timeline_events ADD COLUMN parser_version TEXT;
            ALTER TABLE timeline_events ADD COLUMN confidence REAL;
            ALTER TABLE timeline_events ADD COLUMN source_attribution TEXT;",
        )
        .unwrap();
        conn
    }

    fn setup_nullable_ts_db() -> rusqlite::Connection {
        let conn = crate::connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE timeline_events (
                id TEXT PRIMARY KEY NOT NULL,
                case_id TEXT NOT NULL DEFAULT '',
                source_object_id TEXT NOT NULL,
                event_type TEXT NOT NULL,
                ts TEXT,
                title TEXT NOT NULL,
                description TEXT NOT NULL DEFAULT '',
                parser_id TEXT,
                parser_version TEXT,
                confidence REAL,
                source_attribution TEXT,
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
            parser_id: None,
            parser_version: None,
            confidence: None,
            source_attribution: None,
            attrs: BTreeMap::new(),
        }
    }

    fn ids(events: &[TimelineEvent]) -> Vec<String> {
        events.iter().map(|event| event.id.0.clone()).collect()
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
    fn query_large_unfiltered_uses_deterministic_timestamp_and_id_order() {
        let conn = setup_db();
        let repo = TimelineRepo::new(&conn);
        let mut events = Vec::new();
        for idx in (0..150).rev() {
            let day = (idx % 5) + 1;
            events.push(make_event(
                &format!("event-{idx:03}"),
                "file_modify",
                &format!("2025-01-{day:02}T00:00:00Z"),
            ));
        }
        repo.insert_batch(&events).unwrap();

        let results = repo.query(0, 200).unwrap();
        let result_ids = ids(&results);
        let mut expected = events.clone();
        expected.sort_by(|left, right| {
            right
                .timestamp
                .cmp(&left.timestamp)
                .then_with(|| left.id.0.cmp(&right.id.0))
        });

        assert_eq!(result_ids, ids(&expected));
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
    fn query_filtered_for_case_type_and_time_is_deterministic() {
        let conn = setup_db();
        let repo = TimelineRepo::new(&conn);
        let events = [
            make_event("a-other-case", "file_create", "2025-01-04T00:00:00Z"),
            make_event("b-outside-range", "file_create", "2025-01-01T00:00:00Z"),
            make_event("c-wrong-type", "file_delete", "2025-01-03T00:00:00Z"),
            make_event("d-match-later", "file_create", "2025-01-03T00:00:00Z"),
            make_event("c-match-later", "file_create", "2025-01-03T00:00:00Z"),
            make_event("e-match-earlier", "file_create", "2025-01-02T00:00:00Z"),
        ];
        repo.insert_batch_with_case(&events[0..1], "case-2")
            .unwrap();
        repo.insert_batch_with_case(&events[1..], "case-1").unwrap();

        let results = repo
            .query_filtered_for_case(
                0,
                10,
                Some("case-1"),
                Some("2025-01-02T00:00:00+00:00"),
                Some("2025-01-03T23:59:59+00:00"),
                Some("file_create"),
            )
            .unwrap();
        let count = repo
            .count_filtered_for_case(
                Some("case-1"),
                Some("2025-01-02T00:00:00+00:00"),
                Some("2025-01-03T23:59:59+00:00"),
                Some("file_create"),
            )
            .unwrap();

        assert_eq!(count, 3);
        assert_eq!(
            ids(&results),
            vec![
                "c-match-later".to_string(),
                "d-match-later".to_string(),
                "e-match-earlier".to_string(),
            ]
        );
    }

    #[test]
    fn identical_timestamp_pagination_is_stable() {
        let conn = setup_db();
        let repo = TimelineRepo::new(&conn);
        let events = vec![
            make_event("same-04", "file_modify", "2025-01-01T00:00:00Z"),
            make_event("same-02", "file_modify", "2025-01-01T00:00:00Z"),
            make_event("same-05", "file_modify", "2025-01-01T00:00:00Z"),
            make_event("same-01", "file_modify", "2025-01-01T00:00:00Z"),
            make_event("same-03", "file_modify", "2025-01-01T00:00:00Z"),
        ];
        repo.insert_batch(&events).unwrap();

        let page_one = repo.query(0, 2).unwrap();
        let page_two = repo.query(2, 2).unwrap();
        let page_three = repo.query(4, 2).unwrap();
        let mut paged = ids(&page_one);
        paged.extend(ids(&page_two));
        paged.extend(ids(&page_three));

        assert_eq!(
            paged,
            vec![
                "same-01".to_string(),
                "same-02".to_string(),
                "same-03".to_string(),
                "same-04".to_string(),
                "same-05".to_string(),
            ]
        );
    }

    #[test]
    fn missing_timestamp_rows_sort_last_and_load_as_default_timestamp() {
        let conn = setup_nullable_ts_db();
        conn.execute_batch(
            "INSERT INTO timeline_events (id, source_object_id, event_type, ts, title, description, attrs)
             VALUES
                ('missing-ts', 'src-1', 'file_modify', NULL, 'Missing', '', '{}'),
                ('has-ts', 'src-1', 'file_modify', '2025-01-01T00:00:00Z', 'Has timestamp', '', '{}');",
        )
        .unwrap();
        let repo = TimelineRepo::new(&conn);

        let results = repo.query(0, 10).unwrap();

        assert_eq!(
            ids(&results),
            vec!["has-ts".to_string(), "missing-ts".to_string()]
        );
        assert_eq!(
            results[1].timestamp,
            chrono::DateTime::<chrono::Utc>::default()
        );
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

    #[test]
    fn timeline_provenance_round_trips() {
        let conn = setup_db();
        let repo = TimelineRepo::new(&conn);
        let mut event = make_event("e1", "file_modify", "2025-01-01T00:00:00Z");
        event.parser_id = Some("timeline.macb".to_string());
        event.parser_version = Some("1.2.3".to_string());
        event.confidence = Some(0.87);
        event.source_attribution = Some("modified_at".to_string());

        repo.insert_batch(&[event]).unwrap();

        let rows = repo.query(0, 10).unwrap();
        assert_eq!(rows[0].parser_id.as_deref(), Some("timeline.macb"));
        assert_eq!(rows[0].parser_version.as_deref(), Some("1.2.3"));
        assert_eq!(rows[0].confidence, Some(0.87));
        assert_eq!(rows[0].source_attribution.as_deref(), Some("modified_at"));
    }

    #[test]
    fn timeline_missing_confidence_loads_as_unknown() {
        let conn = setup_legacy_db();
        conn.execute(
            "INSERT INTO timeline_events (id, source_object_id, event_type, ts, title, description, attrs)
             VALUES ('e1', 'src-1', 'file_modify', '2025-01-01T00:00:00Z', 'Modified', '', '{}')",
            [],
        )
        .unwrap();
        let repo = TimelineRepo::new(&conn);

        let rows = repo.query(0, 10).unwrap();

        assert_eq!(rows.len(), 1);
        assert!(rows[0].parser_id.is_none());
        assert!(rows[0].parser_version.is_none());
        assert!(rows[0].confidence.is_none());
        assert!(rows[0].source_attribution.is_none());
    }
}
