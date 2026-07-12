use crate::connection::DbResult;
use crate::sql_builder::ClauseBuilder;
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
        let mut builder = ClauseBuilder::new();
        if let Some(case_id) = case_id {
            builder.push_eq("case_id", case_id.to_string());
        }
        if let Some(start) = time_start {
            builder.push_cmp("ts", ">=", start.to_string());
        }
        if let Some(end) = time_end {
            builder.push_cmp("ts", "<=", end.to_string());
        }
        if let Some(et) = event_type {
            builder.push_eq("event_type", et.to_string());
        }
        let limit_param = builder.push_param(limit);
        let offset_param = builder.push_param(offset);

        let sql = format!(
            "SELECT {TIMELINE_SELECT_COLUMNS} FROM timeline_events {} {TIMELINE_ORDER_BY} LIMIT ?{limit_param} OFFSET ?{offset_param}",
            builder.where_clause(),
        );

        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(builder.param_refs().as_slice(), row_to_timeline_event)?;
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
        let mut builder = ClauseBuilder::new();
        if let Some(case_id) = case_id {
            builder.push_eq("case_id", case_id.to_string());
        }
        if let Some(start) = time_start {
            builder.push_cmp("ts", ">=", start.to_string());
        }
        if let Some(end) = time_end {
            builder.push_cmp("ts", "<=", end.to_string());
        }
        if let Some(et) = event_type {
            builder.push_eq("event_type", et.to_string());
        }

        let sql = format!(
            "SELECT COUNT(*) FROM timeline_events {}",
            builder.where_clause()
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let n: i64 = stmt.query_row(builder.param_refs().as_slice(), |r| r.get(0))?;
        Ok(n as u64)
    }

    pub fn find_by_id(&self, event_id: &str) -> DbResult<Option<TimelineEvent>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {TIMELINE_SELECT_COLUMNS} FROM timeline_events WHERE id = ?1"
        ))?;
        let result = stmt.query_row(params![event_id], row_to_timeline_event);
        match result {
            Ok(event) => Ok(Some(event)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(error) => Err(error.into()),
        }
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
#[path = "../../tests/unit/repositories/timeline_repo.rs"]
mod tests;
