use crate::connection::DbResult;
use crate::repositories::source_meta_repo::{SourceMetaRepo, TIMELINE_CURSOR_REVISION_KEY};
use domain::TimelineEvent;
use rusqlite::{params, Connection};

const TIMELINE_SELECT_COLUMNS: &str = "id, source_object_id, event_type, ts, title, description, parser_id, parser_version, confidence, source_attribution, attrs";
const TIMELINE_ORDER_BY: &str = "ORDER BY ts DESC, id ASC";

mod analysis_outputs;
mod bulk_indexes;
mod filtering;
mod mapping;
mod native;
mod persistence;
pub use bulk_indexes::with_deferred_empty_indexes;
use filtering::{count_timeline_rows, filtered_timeline_clauses};
use mapping::{row_to_timeline_cursor_row, row_to_timeline_event};
use native::events as fjall_events;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimelineSortKey {
    pub timestamp: Option<String>,
    pub event_id: String,
}

#[derive(Debug, Clone)]
pub struct TimelineCursorRow {
    pub event: TimelineEvent,
    pub sort_key: TimelineSortKey,
}

pub struct TimelineRepo<'a> {
    conn: &'a Connection,
}

impl<'a> TimelineRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn delete_analysis_outputs_in_transaction(
        &self,
        source_object_id: &str,
        producer_prefix: &str,
    ) -> DbResult<usize> {
        if let Some(events) = fjall_events(self.conn)? {
            let store = crate::source_fjall::open_store_for_write(self.conn)?
                .expect("Fjall source store is configured");
            let ids = events
                .into_iter()
                .filter(|event| {
                    event.source_object_id == source_object_id
                        && event
                            .parser_id
                            .as_deref()
                            .is_some_and(|id| id.starts_with(producer_prefix))
                })
                .map(|event| event.id.0)
                .collect::<Vec<_>>();
            return persistence_fjall::TimelineRecordStore::from_source_store((*store).clone())
                .delete_batch(
                    &crate::source_fjall::source_id(self.conn)?
                        .expect("source identity is configured"),
                    &ids,
                    persistence_fjall::PersistMode::SyncAll,
                )
                .map_err(|error| crate::connection::DbError::System(error.to_string()));
        }
        let deleted = self.conn.execute(
            "DELETE FROM timeline_events
                 WHERE source_object_id = ?1
                   AND parser_id LIKE ?2",
            params![source_object_id, format!("{producer_prefix}%")],
        )?;
        if deleted > 0 {
            SourceMetaRepo::new(self.conn).bump_revision(TIMELINE_CURSOR_REVISION_KEY)?;
        }
        Ok(deleted)
    }

    pub fn list_analysis_outputs(
        &self,
        source_object_id: &str,
        producer_prefix: &str,
    ) -> DbResult<Vec<TimelineEvent>> {
        if let Some(events) = fjall_events(self.conn)? {
            return Ok(events
                .into_iter()
                .filter(|event| {
                    event.source_object_id == source_object_id
                        && event
                            .parser_id
                            .as_deref()
                            .is_some_and(|id| id.starts_with(producer_prefix))
                })
                .collect());
        }
        let mut statement = self.conn.prepare(&format!(
            "SELECT {TIMELINE_SELECT_COLUMNS}
             FROM timeline_events
             WHERE source_object_id = ?1
               AND parser_id LIKE ?2
             ORDER BY rowid ASC"
        ))?;
        let rows = statement.query_map(
            params![source_object_id, format!("{producer_prefix}%")],
            row_to_timeline_event,
        )?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn count_analysis_outputs(
        &self,
        source_object_id: &str,
        producer_prefix: &str,
    ) -> DbResult<u64> {
        if let Some(events) = fjall_events(self.conn)? {
            return Ok(events
                .into_iter()
                .filter(|event| {
                    event.source_object_id == source_object_id
                        && event
                            .parser_id
                            .as_deref()
                            .is_some_and(|id| id.starts_with(producer_prefix))
                })
                .count() as u64);
        }
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*)
             FROM timeline_events
             WHERE source_object_id = ?1
               AND parser_id LIKE ?2",
            params![source_object_id, format!("{producer_prefix}%")],
            |row| row.get(0),
        )?;
        Ok(count.max(0) as u64)
    }

    pub fn list_analysis_outputs_for_prefixes(
        &self,
        producer_prefixes: &[&str],
    ) -> DbResult<Vec<TimelineEvent>> {
        if producer_prefixes.is_empty() {
            return Ok(Vec::new());
        }
        let predicates = (1..=producer_prefixes.len())
            .map(|index| format!("parser_id LIKE ?{index}"))
            .collect::<Vec<_>>()
            .join(" OR ");
        let sql = format!(
            "SELECT {TIMELINE_SELECT_COLUMNS}
             FROM timeline_events
             WHERE parser_id IS NOT NULL
               AND ({predicates})
             ORDER BY rowid ASC"
        );
        let patterns = producer_prefixes
            .iter()
            .map(|prefix| format!("{prefix}%"))
            .collect::<Vec<_>>();
        let mut statement = self.conn.prepare(&sql)?;
        let rows =
            statement.query_map(rusqlite::params_from_iter(patterns), row_to_timeline_event)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn list_analysis_outputs_for_sources(
        &self,
        source_object_ids: &[&str],
        producer_prefix: &str,
    ) -> DbResult<Vec<TimelineEvent>> {
        if source_object_ids.is_empty() {
            return Ok(Vec::new());
        }
        let placeholders = (1..=source_object_ids.len())
            .map(|index| format!("?{index}"))
            .collect::<Vec<_>>()
            .join(", ");
        let prefix_parameter = source_object_ids.len() + 1;
        let sql = format!(
            "SELECT {TIMELINE_SELECT_COLUMNS}
             FROM timeline_events
             WHERE source_object_id IN ({placeholders})
               AND parser_id LIKE ?{prefix_parameter}
             ORDER BY source_object_id ASC, rowid ASC"
        );
        let mut parameters = source_object_ids
            .iter()
            .map(|source_id| (*source_id).to_string())
            .collect::<Vec<_>>();
        parameters.push(format!("{producer_prefix}%"));
        let mut statement = self.conn.prepare(&sql)?;
        let rows = statement.query_map(
            rusqlite::params_from_iter(parameters),
            row_to_timeline_event,
        )?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn visit_analysis_outputs_for_sources<F>(
        &self,
        source_object_ids: &[&str],
        producer_prefix: &str,
        mut visit: F,
    ) -> DbResult<()>
    where
        F: FnMut(TimelineEvent) -> DbResult<()>,
    {
        if source_object_ids.is_empty() {
            return Ok(());
        }
        let placeholders = (1..=source_object_ids.len())
            .map(|index| format!("?{index}"))
            .collect::<Vec<_>>()
            .join(", ");
        let prefix_parameter = source_object_ids.len() + 1;
        let sql = format!(
            "SELECT {TIMELINE_SELECT_COLUMNS} FROM timeline_events
             WHERE source_object_id IN ({placeholders})
               AND parser_id LIKE ?{prefix_parameter}
             ORDER BY source_object_id ASC, rowid ASC"
        );
        let mut parameters = source_object_ids
            .iter()
            .map(|source_id| (*source_id).to_string())
            .collect::<Vec<_>>();
        parameters.push(format!("{producer_prefix}%"));
        let mut statement = self.conn.prepare(&sql)?;
        let rows = statement.query_map(
            rusqlite::params_from_iter(parameters),
            row_to_timeline_event,
        )?;
        for row in rows {
            visit(row?)?;
        }
        Ok(())
    }

    pub fn query(&self, offset: u64, limit: u32) -> DbResult<Vec<TimelineEvent>> {
        if let Some(mut events) = fjall_events(self.conn)? {
            events.sort_by(|left, right| {
                right
                    .timestamp
                    .cmp(&left.timestamp)
                    .then_with(|| left.id.0.cmp(&right.id.0))
            });
            return Ok(events
                .into_iter()
                .skip(offset as usize)
                .take(limit as usize)
                .collect());
        }
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

    /// Query timeline events with optional filtering.
    pub fn query_filtered(
        &self,
        offset: u64,
        limit: u32,
        time_start: Option<&str>,
        time_end: Option<&str>,
        event_type: Option<&str>,
    ) -> DbResult<Vec<TimelineEvent>> {
        if let Some(mut events) = fjall_events(self.conn)? {
            events.retain(|event| {
                event_type.is_none_or(|value| event.event_type == value)
                    && time_start.is_none_or(|value| event.timestamp.to_rfc3339().as_str() >= value)
                    && time_end.is_none_or(|value| event.timestamp.to_rfc3339().as_str() <= value)
            });
            events.sort_by(|left, right| {
                right
                    .timestamp
                    .cmp(&left.timestamp)
                    .then_with(|| left.id.0.cmp(&right.id.0))
            });
            return Ok(events
                .into_iter()
                .skip(offset as usize)
                .take(limit as usize)
                .collect());
        }
        self.query_filtered_for_case(offset, limit, None, time_start, time_end, event_type)
    }

    pub fn query_filtered_after(
        &self,
        after: Option<&TimelineSortKey>,
        limit: u32,
        time_start: Option<&str>,
        time_end: Option<&str>,
        event_type: Option<&str>,
    ) -> DbResult<Vec<TimelineCursorRow>> {
        self.query_filtered_after_with_snapshot(
            after, None, limit, time_start, time_end, event_type,
        )
    }

    pub fn query_filtered_after_at_snapshot(
        &self,
        after: Option<&TimelineSortKey>,
        snapshot_high_water: i64,
        limit: u32,
        time_start: Option<&str>,
        time_end: Option<&str>,
        event_type: Option<&str>,
    ) -> DbResult<Vec<TimelineCursorRow>> {
        self.query_filtered_after_with_snapshot(
            after,
            Some(snapshot_high_water),
            limit,
            time_start,
            time_end,
            event_type,
        )
    }

    fn query_filtered_after_with_snapshot(
        &self,
        after: Option<&TimelineSortKey>,
        snapshot_high_water: Option<i64>,
        limit: u32,
        time_start: Option<&str>,
        time_end: Option<&str>,
        event_type: Option<&str>,
    ) -> DbResult<Vec<TimelineCursorRow>> {
        let mut builder = filtered_timeline_clauses(None, time_start, time_end, event_type);
        if let Some(high_water) = snapshot_high_water {
            builder.push_cmp("rowid", "<=", high_water);
        }
        if let Some(after) = after {
            match &after.timestamp {
                Some(timestamp) => {
                    let timestamp_param = builder.next_param();
                    let event_id_param = timestamp_param + 1;
                    builder.push_raw(
                        format!(
                            "(ts < ?{timestamp_param} OR ts IS NULL OR (ts = ?{timestamp_param} AND id > ?{event_id_param}))"
                        ),
                        vec![timestamp.clone(), after.event_id.clone()],
                    );
                }
                None => {
                    let event_id_param = builder.next_param();
                    builder.push_raw(
                        format!("(ts IS NULL AND id > ?{event_id_param})"),
                        vec![after.event_id.clone()],
                    );
                }
            }
        }
        let limit_param = builder.push_param(limit);
        let sql = format!(
            "SELECT {TIMELINE_SELECT_COLUMNS} FROM timeline_events {} {TIMELINE_ORDER_BY} LIMIT ?{limit_param}",
            builder.where_clause(),
        );
        let mut statement = self.conn.prepare(&sql)?;
        let rows =
            statement.query_map(builder.param_refs().as_slice(), row_to_timeline_cursor_row)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn snapshot_high_water(&self) -> DbResult<i64> {
        if let Some(events) = fjall_events(self.conn)? {
            return i64::try_from(events.len()).map_err(|_| {
                crate::connection::DbError::System(
                    "timeline count exceeds SQLite integer range".into(),
                )
            });
        }
        self.conn
            .query_row(
                "SELECT COALESCE(MAX(rowid), 0) FROM timeline_events",
                [],
                |row| row.get(0),
            )
            .map_err(Into::into)
    }

    pub fn cursor_revision(&self) -> DbResult<u64> {
        if let Some(store) = crate::source_fjall::open_store(self.conn)? {
            return persistence_fjall::TimelineRecordStore::from_source_store((*store).clone())
                .cursor_revision(
                    &persistence_fjall::TimelineRecordStore::from_source_store((*store).clone())
                        .read_snapshot(),
                )
                .map_err(|error| crate::connection::DbError::System(error.to_string()));
        }
        SourceMetaRepo::new(self.conn).read_revision(TIMELINE_CURSOR_REVISION_KEY)
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
        let mut builder = filtered_timeline_clauses(case_id, time_start, time_end, event_type);
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
        if let Some(events) = fjall_events(self.conn)? {
            return Ok(events
                .into_iter()
                .filter(|event| {
                    event_type.is_none_or(|value| event.event_type == value)
                        && time_start
                            .is_none_or(|value| event.timestamp.to_rfc3339().as_str() >= value)
                        && time_end
                            .is_none_or(|value| event.timestamp.to_rfc3339().as_str() <= value)
                })
                .count() as u64);
        }
        self.count_filtered_for_case(None, time_start, time_end, event_type)
    }

    pub fn count_filtered_at_snapshot(
        &self,
        snapshot_high_water: i64,
        time_start: Option<&str>,
        time_end: Option<&str>,
        event_type: Option<&str>,
    ) -> DbResult<u64> {
        let mut builder = filtered_timeline_clauses(None, time_start, time_end, event_type);
        builder.push_cmp("rowid", "<=", snapshot_high_water);
        count_timeline_rows(self.conn, builder)
    }

    pub fn count_filtered_for_case(
        &self,
        case_id: Option<&str>,
        time_start: Option<&str>,
        time_end: Option<&str>,
        event_type: Option<&str>,
    ) -> DbResult<u64> {
        count_timeline_rows(
            self.conn,
            filtered_timeline_clauses(case_id, time_start, time_end, event_type),
        )
    }

    pub fn find_by_id(&self, event_id: &str) -> DbResult<Option<TimelineEvent>> {
        if let Some(events) = fjall_events(self.conn)? {
            return Ok(events.into_iter().find(|event| event.id.0 == event_id));
        }
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

#[cfg(test)]
#[path = "../../tests/unit/repositories/timeline_repo.rs"]
mod tests;
