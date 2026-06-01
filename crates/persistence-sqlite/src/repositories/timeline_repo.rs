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
