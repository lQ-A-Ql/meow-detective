use crate::connection::{DbError, DbResult};
use rusqlite::Connection;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimelineFacetSummary {
    pub total: u64,
    pub min_epoch: Option<i64>,
    pub max_epoch: Option<i64>,
}

pub struct TimelineFacetsRepo<'a> {
    _conn: &'a Connection,
}

impl<'a> TimelineFacetsRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { _conn: conn }
    }

    pub fn summary(
        &self,
        _time_start: Option<i64>,
        _time_end: Option<i64>,
        _event_type: Option<&str>,
    ) -> DbResult<TimelineFacetSummary> {
        Err(DbError::System("source detail storage is missing".into()))
    }

    pub fn event_type_counts(
        &self,
        _time_start: Option<i64>,
        _time_end: Option<i64>,
        _event_type: Option<&str>,
    ) -> DbResult<Vec<(String, u64)>> {
        Err(DbError::System("source detail storage is missing".into()))
    }

    pub fn bucket_counts(
        &self,
        _time_start: Option<i64>,
        _time_end: Option<i64>,
        _event_type: Option<&str>,
        _min_epoch: i64,
        _max_epoch: i64,
        _bucket_count: u32,
    ) -> DbResult<Vec<(u32, u64)>> {
        Err(DbError::System("source detail storage is missing".into()))
    }
}
