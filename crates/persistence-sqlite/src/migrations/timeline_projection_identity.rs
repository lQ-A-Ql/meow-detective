use crate::connection::DbResult;
use rusqlite::Connection;

pub(super) fn add_timeline_projection_identity(conn: &Connection, sql: &str) -> DbResult<()> {
    conn.execute_batch(sql)?;
    let column_exists: bool = conn.query_row(
        "SELECT COUNT(*) > 0
         FROM pragma_table_info('timeline_projection_meta')
         WHERE name = 'input_identity'",
        [],
        |row| row.get(0),
    )?;
    if !column_exists {
        conn.execute_batch(
            "ALTER TABLE timeline_projection_meta
             ADD COLUMN input_identity TEXT NOT NULL DEFAULT ''",
        )?;
    }
    Ok(())
}
