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
        let tx = self.conn.unchecked_transaction()?;
        {
            let mut stmt = tx.prepare_cached(
                "INSERT INTO timeline_events (id, case_id, source_object_id, event_type, ts, title, description, attrs)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            )?;
            for ev in events {
                stmt.execute(params![
                    ev.id.0,
                    "",
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
}
