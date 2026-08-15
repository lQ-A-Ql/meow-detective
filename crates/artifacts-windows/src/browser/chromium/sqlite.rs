use rusqlite::Connection;
use std::io::Write;

/// Open a SQLite connection from an in-memory byte slice by writing to a
/// temporary file. `rusqlite` opens the temporary copy read-write; the
/// original evidence remains untouched.
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
