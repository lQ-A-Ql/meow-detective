use rusqlite::{Connection, Statement};

use crate::connection::{DbError, DbResult};

const SQLITE_FALLBACK_VARIABLE_LIMIT: usize = 999;
const TARGET_BATCH_PARAMETERS: usize = 8_192;
const MAX_BATCH_ROWS: usize = 1_024;

pub(super) fn insert_rows<T>(
    conn: &Connection,
    insert_prefix: &str,
    columns_per_row: usize,
    records: &[T],
    mut execute: impl FnMut(&mut Statement<'_>, &[T]) -> rusqlite::Result<usize>,
) -> DbResult<()> {
    if records.is_empty() {
        return Ok(());
    }
    if columns_per_row == 0 {
        return Err(DbError::System(
            "batched SQLite insert requires at least one column".to_string(),
        ));
    }
    let sqlite_limit = sqlite_variable_limit(conn)?;
    if columns_per_row > sqlite_limit {
        return Err(DbError::System(
            "batched SQLite insert exceeds the variable limit".to_string(),
        ));
    }
    let parameter_limit = sqlite_limit.min(TARGET_BATCH_PARAMETERS);
    let batch_rows = (parameter_limit / columns_per_row).clamp(1, MAX_BATCH_ROWS);
    let full_sql = insert_sql(insert_prefix, columns_per_row, batch_rows);
    let tail_rows = records.len() % batch_rows;
    let tail_sql = (tail_rows > 0).then(|| insert_sql(insert_prefix, columns_per_row, tail_rows));

    for records in records.chunks(batch_rows) {
        let sql = if records.len() == batch_rows {
            &full_sql
        } else {
            tail_sql.as_deref().ok_or_else(|| {
                DbError::System("batched SQLite insert tail was not prepared".to_string())
            })?
        };
        let mut statement = conn.prepare_cached(sql)?;
        execute(&mut statement, records)?;
    }
    Ok(())
}

fn sqlite_variable_limit(conn: &Connection) -> DbResult<usize> {
    let mut statement = conn.prepare("PRAGMA compile_options")?;
    let options = statement.query_map([], |row| row.get::<_, String>(0))?;
    for option in options {
        let option = option?;
        if let Some(value) = option.strip_prefix("MAX_VARIABLE_NUMBER=") {
            return value.parse::<usize>().map_err(|_| {
                DbError::System("SQLite MAX_VARIABLE_NUMBER is not numeric".to_string())
            });
        }
    }
    Ok(SQLITE_FALLBACK_VARIABLE_LIMIT)
}

fn insert_sql(insert_prefix: &str, columns_per_row: usize, rows: usize) -> String {
    let row = format!("({})", vec!["?"; columns_per_row].join(", "));
    format!("{insert_prefix}{}", vec![row; rows].join(", "))
}
