use super::{schema::Schema, BrowserFamily, HistoryKind, HistoryRow, VisitError};
use crate::BrowserError;
use rusqlite::Connection;
use std::sync::atomic::{AtomicBool, Ordering};

pub fn visit_history<E>(
    connection: &Connection,
    family: BrowserFamily,
    cancel: &AtomicBool,
    mut visit: impl FnMut(HistoryRow) -> std::result::Result<(), E>,
) -> std::result::Result<(), VisitError<E>> {
    let schema = Schema::read(connection, family)?;
    if schema.has_visits {
        pages(connection, &schema, HistoryKind::Visit, cancel, &mut visit)?;
    }
    pages(
        connection,
        &schema,
        HistoryKind::UrlSummary,
        cancel,
        &mut visit,
    )
}

fn pages<E>(
    connection: &Connection,
    schema: &Schema,
    kind: HistoryKind,
    cancel: &AtomicBool,
    visit: &mut impl FnMut(HistoryRow) -> std::result::Result<(), E>,
) -> std::result::Result<(), VisitError<E>> {
    let mut cursor = i64::MIN;
    let mut inclusive = true;
    loop {
        if cancel.load(Ordering::Acquire) {
            return Err(BrowserError::Cancelled.into());
        }
        crate::sqlite::refresh_query_budget(connection);
        let sql = schema.query(kind, inclusive);
        let mut statement = connection
            .prepare_cached(&sql)
            .map_err(BrowserError::from)?;
        let mut rows = statement.query([cursor]).map_err(BrowserError::from)?;
        let mut count = 0usize;
        while let Some(row) = rows.next().map_err(BrowserError::from)? {
            if cancel.load(Ordering::Acquire) {
                return Err(BrowserError::Cancelled.into());
            }
            let entry = decode(row, schema, kind).map_err(BrowserError::from)?;
            if (count > 0 || !inclusive) && entry.row_id <= cursor {
                return Err(BrowserError::Parse("history row identity is ambiguous".into()).into());
            }
            cursor = entry.row_id;
            visit(entry).map_err(VisitError::Sink)?;
            count += 1;
        }
        if count < 1024 || cursor == i64::MAX {
            return Ok(());
        }
        inclusive = false;
    }
}

fn decode(
    row: &rusqlite::Row<'_>,
    schema: &Schema,
    kind: HistoryKind,
) -> rusqlite::Result<HistoryRow> {
    Ok(HistoryRow {
        row_id: row.get(0)?,
        visit_id: row.get(1)?,
        url_id: row.get(2)?,
        url: row.get(3)?,
        title: row.get(4)?,
        visit_count: row.get(5)?,
        raw_time: row.get(6)?,
        clock: schema.clock,
        kind,
    })
}
