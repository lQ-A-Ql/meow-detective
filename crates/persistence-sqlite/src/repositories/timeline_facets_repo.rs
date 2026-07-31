use crate::connection::DbResult;
use rusqlite::{params_from_iter, types::Value, Connection};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimelineFacetSummary {
    pub total: u64,
    pub min_epoch: Option<i64>,
    pub max_epoch: Option<i64>,
}

pub struct TimelineFacetsRepo<'a> {
    conn: &'a Connection,
}

impl<'a> TimelineFacetsRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn summary(
        &self,
        time_start: Option<i64>,
        time_end: Option<i64>,
        event_type: Option<&str>,
    ) -> DbResult<TimelineFacetSummary> {
        let (where_clause, values) = filtered_clause(time_start, time_end, event_type);
        let sql = format!(
            "SELECT COUNT(*), MIN(unixepoch(ts)), MAX(unixepoch(ts))
             FROM timeline_events {where_clause}"
        );
        let mut statement = self.conn.prepare(&sql)?;
        let (total, min_epoch, max_epoch): (i64, Option<i64>, Option<i64>) = statement
            .query_row(params_from_iter(values.iter()), |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })?;
        Ok(TimelineFacetSummary {
            total: u64::try_from(total).unwrap_or_default(),
            min_epoch,
            max_epoch,
        })
    }

    pub fn event_type_counts(
        &self,
        time_start: Option<i64>,
        time_end: Option<i64>,
        event_type: Option<&str>,
    ) -> DbResult<Vec<(String, u64)>> {
        let (where_clause, values) = filtered_clause(time_start, time_end, event_type);
        let sql = format!(
            "SELECT event_type, COUNT(*)
             FROM timeline_events {where_clause}
             GROUP BY event_type
             ORDER BY event_type ASC"
        );
        let mut statement = self.conn.prepare(&sql)?;
        let rows = statement.query_map(params_from_iter(values.iter()), |row| {
            let count: i64 = row.get(1)?;
            Ok((row.get(0)?, u64::try_from(count).unwrap_or_default()))
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn bucket_counts(
        &self,
        time_start: Option<i64>,
        time_end: Option<i64>,
        event_type: Option<&str>,
        min_epoch: i64,
        max_epoch: i64,
        bucket_count: u32,
    ) -> DbResult<Vec<(u32, u64)>> {
        let inclusive_span = max_epoch.saturating_sub(min_epoch).saturating_add(1).max(1);
        let (where_clause, mut values) = filtered_clause(time_start, time_end, event_type);
        let min_param = values.len() + 1;
        let bucket_param = values.len() + 2;
        let span_param = values.len() + 3;
        values.push(Value::Integer(min_epoch));
        values.push(Value::Integer(i64::from(bucket_count)));
        values.push(Value::Integer(inclusive_span));
        let bucket_expression = format!(
            "MIN(?{bucket_param} - 1,
                 CAST(((unixepoch(ts) - ?{min_param}) * 1.0 * ?{bucket_param})
                      / ?{span_param} AS INTEGER))"
        );
        let sql = format!(
            "SELECT {bucket_expression} AS bucket, COUNT(*)
             FROM timeline_events {where_clause}
             GROUP BY bucket
             ORDER BY bucket ASC"
        );
        let mut statement = self.conn.prepare(&sql)?;
        let rows = statement.query_map(params_from_iter(values.iter()), |row| {
            let bucket: i64 = row.get(0)?;
            let count: i64 = row.get(1)?;
            Ok((
                u32::try_from(bucket).unwrap_or_default(),
                u64::try_from(count).unwrap_or_default(),
            ))
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }
}

fn filtered_clause(
    time_start: Option<i64>,
    time_end: Option<i64>,
    event_type: Option<&str>,
) -> (String, Vec<Value>) {
    let mut clauses = vec!["unixepoch(ts) IS NOT NULL".to_string()];
    let mut values = Vec::new();
    if let Some(start) = time_start {
        values.push(Value::Integer(start));
        clauses.push(format!("unixepoch(ts) >= ?{}", values.len()));
    }
    if let Some(end) = time_end {
        values.push(Value::Integer(end));
        clauses.push(format!("unixepoch(ts) <= ?{}", values.len()));
    }
    if let Some(event_type) = event_type {
        values.push(Value::Text(event_type.to_string()));
        clauses.push(format!("event_type = ?{}", values.len()));
    }
    (format!("WHERE {}", clauses.join(" AND ")), values)
}
