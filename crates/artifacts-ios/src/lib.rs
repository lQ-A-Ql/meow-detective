pub mod backup;
pub mod calls;
pub mod contacts;
pub mod messages;
pub mod notes;
pub mod photos;
pub mod safari;

use chrono::TimeZone;
use thiserror::Error;

/// Unified error type for iOS artifact parsing.
#[derive(Error, Debug)]
pub enum IosArtifactError {
    #[error("SQLite error: {0}")]
    Sqlite(String),
    #[error("Parse error: {0}")]
    Parse(String),
    #[error("IO error: {0}")]
    Io(String),
}

impl From<rusqlite::Error> for IosArtifactError {
    fn from(e: rusqlite::Error) -> Self {
        IosArtifactError::Sqlite(e.to_string())
    }
}

impl From<std::io::Error> for IosArtifactError {
    fn from(e: std::io::Error) -> Self {
        IosArtifactError::Io(e.to_string())
    }
}

/// Open a SQLite connection from an in-memory byte slice by writing to a
/// temporary file. `rusqlite` (bundled) opens the db read-write; we use a
/// temp file that is automatically cleaned up when the connection is dropped.
pub(crate) fn open_sqlite_from_bytes(
    data: &[u8],
) -> Result<(rusqlite::Connection, tempfile::NamedTempFile), IosArtifactError> {
    let mut tmp = tempfile::NamedTempFile::new()?;
    std::io::Write::write_all(&mut tmp, data)?;
    std::io::Write::flush(&mut tmp)?;
    let conn = rusqlite::Connection::open(tmp.path())?;
    Ok((conn, tmp))
}

/// Convert an iOS CoreData / CFAbsoluteTime timestamp (seconds since
/// 2001-01-01 00:00:00 UTC) to a chrono `DateTime<Utc>`.
pub(crate) fn core_data_time_to_dt(seconds: f64) -> Option<chrono::DateTime<chrono::Utc>> {
    if seconds <= 0.0 {
        return None;
    }
    let unix_secs = seconds - 978_307_200.0; // 2001-01-01 → 1970-01-01 offset
    chrono::Utc
        .timestamp_opt(
            unix_secs as i64,
            ((unix_secs % 1.0) * 1_000_000_000.0) as u32,
        )
        .single()
}

/// Try to read a column value from a rusqlite `Row`, returning `None` when the
/// column is missing rather than panicking / erroring.
pub(crate) fn row_get_opt<T: rusqlite::types::FromSql>(
    row: &rusqlite::Row,
    col: &str,
) -> Option<T> {
    row.get(col).ok()
}
