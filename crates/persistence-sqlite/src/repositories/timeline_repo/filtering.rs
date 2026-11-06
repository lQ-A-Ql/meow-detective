use crate::{sql_builder::ClauseBuilder, DbResult};
use rusqlite::Connection;

pub(super) fn count_timeline_rows(conn: &Connection, builder: ClauseBuilder) -> DbResult<u64> {
    let mut statement = conn.prepare(&format!(
        "SELECT COUNT(*) FROM timeline_events {}",
        builder.where_clause()
    ))?;
    let count: u64 = statement.query_row(builder.param_refs().as_slice(), |row| row.get(0))?;
    Ok(count)
}

pub(super) fn filtered_timeline_clauses(
    case_id: Option<&str>,
    start: Option<&str>,
    end: Option<&str>,
    event_type: Option<&str>,
) -> ClauseBuilder {
    let mut builder = ClauseBuilder::new();
    if let Some(case) = case_id {
        builder.push_eq("case_id", case.to_string());
    }
    if let Some(start) = start {
        builder.push_cmp("ts", ">=", start.to_string());
    }
    if let Some(end) = end {
        builder.push_cmp("ts", "<=", end.to_string());
    }
    if let Some(kind) = event_type {
        builder.push_eq("event_type", kind.to_string());
    }
    builder
}
