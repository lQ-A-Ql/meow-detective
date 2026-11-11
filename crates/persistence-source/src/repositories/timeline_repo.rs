use domain::TimelineEvent;
use persistence_sqlite::connection::{DbError, DbResult};
use rusqlite::Connection;

mod analysis_outputs;
mod native;
mod persistence;
mod visit_sources;

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
        if let Some(store) = crate::detail::open_store_for_write(self.conn)? {
            let source_id = crate::detail::source_id(self.conn)?.ok_or_else(|| {
                persistence_sqlite::DbError::System("source identity is missing".into())
            })?;
            return persistence_fjall::TimelineRecordStore::from_source_store((*store).clone())
                .delete_by_source_object_where(
                    &source_id,
                    source_object_id,
                    persistence_fjall::PersistMode::SyncAll,
                    |event| {
                        event
                            .parser_id
                            .as_deref()
                            .is_some_and(|id| id.starts_with(producer_prefix))
                    },
                )
                .map_err(|error| persistence_sqlite::DbError::System(error.to_string()));
        }
        Err(persistence_sqlite::DbError::System(
            "source detail storage is missing".into(),
        ))
    }

    pub fn list_analysis_outputs(
        &self,
        source_object_id: &str,
        producer_prefix: &str,
    ) -> DbResult<Vec<TimelineEvent>> {
        if crate::detail::source_id(self.conn)?.is_none() {
            return Err(persistence_sqlite::DbError::System(
                "source identity is missing".into(),
            ));
        }
        let mut events = Vec::new();
        native::visit_by_source_object(self.conn, source_object_id, |event| {
            if event
                .parser_id
                .as_deref()
                .is_some_and(|id| id.starts_with(producer_prefix))
            {
                events.push(event);
            }
            Ok(())
        })?;
        Ok(events)
    }

    pub fn count_analysis_outputs(
        &self,
        source_object_id: &str,
        producer_prefix: &str,
    ) -> DbResult<u64> {
        if crate::detail::source_id(self.conn)?.is_none() {
            return Err(persistence_sqlite::DbError::System(
                "source identity is missing".into(),
            ));
        }
        let mut count = 0u64;
        native::visit_by_source_object(self.conn, source_object_id, |event| {
            if event
                .parser_id
                .as_deref()
                .is_some_and(|id| id.starts_with(producer_prefix))
            {
                count = count.saturating_add(1);
            }
            Ok(())
        })?;
        Ok(count)
    }

    pub fn list_analysis_outputs_for_prefixes(
        &self,
        producer_prefixes: &[&str],
    ) -> DbResult<Vec<TimelineEvent>> {
        if producer_prefixes.is_empty() {
            return Ok(Vec::new());
        }
        if crate::detail::source_id(self.conn)?.is_none() {
            return Err(persistence_sqlite::DbError::System(
                "source identity is missing".into(),
            ));
        }
        native::matching(self.conn, |event| {
            event.parser_id.as_deref().is_some_and(|id| {
                producer_prefixes
                    .iter()
                    .any(|prefix| id.starts_with(prefix))
            })
        })?
        .ok_or_else(|| {
            persistence_sqlite::DbError::System("source detail storage is missing".into())
        })
    }

    pub fn list_analysis_outputs_for_sources(
        &self,
        source_object_ids: &[&str],
        producer_prefix: &str,
    ) -> DbResult<Vec<TimelineEvent>> {
        if source_object_ids.is_empty() {
            return Ok(Vec::new());
        }
        if crate::detail::source_id(self.conn)?.is_none() {
            return Err(persistence_sqlite::DbError::System(
                "source identity is missing".into(),
            ));
        }
        native::matching(self.conn, |event| {
            source_object_ids.contains(&event.source_object_id.as_str())
                && event
                    .parser_id
                    .as_deref()
                    .is_some_and(|id| id.starts_with(producer_prefix))
        })?
        .ok_or_else(|| {
            persistence_sqlite::DbError::System("source detail storage is missing".into())
        })
    }

    pub fn query(&self, offset: u64, limit: u32) -> DbResult<Vec<TimelineEvent>> {
        native::filtered(self.conn, offset, limit, None, None, None)?.ok_or_else(|| {
            persistence_sqlite::DbError::System("source detail storage is missing".into())
        })
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
        native::filtered(self.conn, offset, limit, time_start, time_end, event_type)?.ok_or_else(
            || persistence_sqlite::DbError::System("source detail storage is missing".into()),
        )
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
        _snapshot_high_water: Option<i64>,
        limit: u32,
        time_start: Option<&str>,
        time_end: Option<&str>,
        event_type: Option<&str>,
    ) -> DbResult<Vec<TimelineCursorRow>> {
        native::filtered_after(self.conn, after, limit, time_start, time_end, event_type)?
            .ok_or_else(|| {
                persistence_sqlite::DbError::System("source detail storage is missing".into())
            })
    }

    pub fn snapshot_high_water(&self) -> DbResult<i64> {
        i64::try_from(self.cursor_revision()?).map_err(|_| {
            persistence_sqlite::connection::DbError::System(
                "timeline revision exceeds cursor range".into(),
            )
        })
    }

    pub fn cursor_revision(&self) -> DbResult<u64> {
        if let Some(store) = crate::detail::open_store(self.conn)? {
            return persistence_fjall::TimelineRecordStore::from_source_store((*store).clone())
                .cursor_revision(
                    &persistence_fjall::TimelineRecordStore::from_source_store((*store).clone())
                        .read_snapshot(),
                )
                .map_err(|error| {
                    persistence_sqlite::connection::DbError::System(error.to_string())
                });
        }
        Err(persistence_sqlite::DbError::System(
            "source detail storage is missing".into(),
        ))
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
        let events = native::filtered(self.conn, offset, limit, time_start, time_end, event_type)?
            .ok_or_else(|| {
                persistence_sqlite::DbError::System("source detail storage is missing".into())
            })?;
        if let Some(case_id) = case_id {
            let owner: String =
                self.conn
                    .query_row("SELECT case_id FROM data_sources LIMIT 1", [], |row| {
                        row.get(0)
                    })?;
            if owner != case_id {
                return Err(DbError::System(
                    "timeline source case owner mismatch".into(),
                ));
            }
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
        native::count_filtered(self.conn, time_start, time_end, event_type)?.ok_or_else(|| {
            persistence_sqlite::DbError::System("source detail storage is missing".into())
        })
    }

    pub fn count_filtered_at_snapshot(
        &self,
        _snapshot_high_water: i64,
        time_start: Option<&str>,
        time_end: Option<&str>,
        event_type: Option<&str>,
    ) -> DbResult<u64> {
        native::count_filtered(self.conn, time_start, time_end, event_type)?.ok_or_else(|| {
            persistence_sqlite::DbError::System("source detail storage is missing".into())
        })
    }

    pub fn count_filtered_for_case(
        &self,
        case_id: Option<&str>,
        time_start: Option<&str>,
        time_end: Option<&str>,
        event_type: Option<&str>,
    ) -> DbResult<u64> {
        let count = native::count_filtered(self.conn, time_start, time_end, event_type)?
            .ok_or_else(|| {
                persistence_sqlite::DbError::System("source detail storage is missing".into())
            })?;
        if let Some(case_id) = case_id {
            let owner: String =
                self.conn
                    .query_row("SELECT case_id FROM data_sources LIMIT 1", [], |row| {
                        row.get(0)
                    })?;
            if owner != case_id {
                return Err(DbError::System(
                    "timeline source case owner mismatch".into(),
                ));
            }
        }
        Ok(count)
    }

    pub fn find_by_id(&self, event_id: &str) -> DbResult<Option<TimelineEvent>> {
        native::get(self.conn, event_id)?.ok_or_else(|| {
            persistence_sqlite::DbError::System("source detail storage is missing".into())
        })
    }
}

#[cfg(test)]
#[path = "../../tests/unit/repositories/timeline_repo.rs"]
mod tests;
