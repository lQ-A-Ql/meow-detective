use rusqlite::Connection;
use std::io::Write;

pub(super) fn open_sqlite_from_bytes(
    data: &[u8],
) -> Result<(Connection, tempfile::NamedTempFile), String> {
    let mut tmp = tempfile::NamedTempFile::new().map_err(|e| format!("tempfile: {}", e))?;
    tmp.write_all(data)
        .map_err(|e| format!("write tempfile: {}", e))?;
    tmp.flush().map_err(|e| format!("flush tempfile: {}", e))?;
    let conn = Connection::open(tmp.path()).map_err(|e| format!("open sqlite: {}", e))?;
    Ok((conn, tmp))
}

pub(super) fn table_exists(conn: &Connection, table: &str) -> bool {
    conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
        [table],
        |row| row.get::<_, i64>(0),
    )
    .map(|count| count > 0)
    .unwrap_or(false)
}
