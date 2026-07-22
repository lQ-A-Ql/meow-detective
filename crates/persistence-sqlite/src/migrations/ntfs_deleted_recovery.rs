use crate::connection::DbResult;
use rusqlite::Connection;

pub(super) fn add_sequence_column(conn: &Connection, sql: &str) -> DbResult<()> {
    let table_exists: bool = conn.query_row(
        "SELECT COUNT(*) > 0
         FROM sqlite_master
         WHERE type = 'table' AND name = 'deleted_file_recoveries'",
        [],
        |row| row.get(0),
    )?;
    if !table_exists {
        return Ok(());
    }
    let column_exists: bool = conn.query_row(
        "SELECT COUNT(*) > 0
         FROM pragma_table_info('deleted_file_recoveries')
         WHERE name = 'mft_sequence'",
        [],
        |row| row.get(0),
    )?;
    if !column_exists {
        conn.execute_batch(sql)?;
    }
    Ok(())
}
