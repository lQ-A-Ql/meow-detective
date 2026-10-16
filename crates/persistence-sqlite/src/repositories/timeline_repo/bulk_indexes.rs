//! Transaction-local index deferral for the first file-activity projection.
use std::time::{Duration, Instant};

use rusqlite::{Connection, Transaction};

use crate::{DbError, DbResult};

/// Preserve constraints while loading an empty timeline, then rebuild its
/// ordinary secondary indexes before releasing the savepoint. On any error
/// (including index creation), dropping the savepoint restores both schema and
/// rows, even if the caller subsequently commits its outer transaction.
///
/// Populated timelines retain their indexes throughout the operation. This
/// requires no durable manifest: the existing transaction also owns the DDL.
pub fn with_deferred_empty_indexes<T, E>(
    transaction: &mut Transaction<'_>,
    operation: impl FnOnce(&Connection) -> Result<T, E>,
) -> Result<(T, Duration), E>
where
    E: From<DbError>,
{
    let savepoint = transaction.savepoint().map_err(DbError::from)?;
    let started = Instant::now();
    let definitions = empty_timeline_indexes(&savepoint)?;
    for (name, _) in &definitions {
        savepoint
            .execute(&format!("DROP INDEX {}", quote_identifier(name)), [])
            .map_err(DbError::from)?;
    }
    let mut index_time = started.elapsed();
    let result = operation(&savepoint)?;
    let started = Instant::now();
    for (_, sql) in definitions {
        savepoint.execute(&sql, []).map_err(DbError::from)?;
    }
    index_time += started.elapsed();
    savepoint.commit().map_err(DbError::from)?;
    Ok((result, index_time))
}

fn empty_timeline_indexes(conn: &Connection) -> DbResult<Vec<(String, String)>> {
    let populated: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM timeline_events LIMIT 1)",
        [],
        |row| row.get(0),
    )?;
    if populated {
        return Ok(Vec::new());
    }
    Ok(conn
        .prepare(
            "SELECT name, sql FROM sqlite_master
             WHERE type='index' AND tbl_name='timeline_events' AND sql IS NOT NULL
               AND name IN (
                   SELECT name FROM pragma_index_list('timeline_events')
                   WHERE \"unique\"=0 AND origin='c'
               )
             ORDER BY name",
        )?
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<Result<Vec<_>, _>>()?)
}

fn quote_identifier(identifier: &str) -> String {
    format!("\"{}\"", identifier.replace('"', "\"\""))
}
