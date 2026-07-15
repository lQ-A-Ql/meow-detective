use rusqlite::{Connection, Statement};

use crate::connection::{DbError, DbResult};

const MAX_BATCH_PARAMETERS: usize = 896;
const MAX_BATCH_ROWS: usize = 128;

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
    let batch_rows = (MAX_BATCH_PARAMETERS / columns_per_row).clamp(1, MAX_BATCH_ROWS);
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

fn insert_sql(insert_prefix: &str, columns_per_row: usize, rows: usize) -> String {
    let row = format!("({})", vec!["?"; columns_per_row].join(", "));
    format!("{insert_prefix}{}", vec![row; rows].join(", "))
}
