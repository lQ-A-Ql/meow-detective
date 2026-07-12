use super::ExtractionOutcome;
use crate::analysis_service::error::AnalysisServiceError;
use rusqlite::{Connection, OpenFlags};
use std::path::PathBuf;
use uuid::Uuid;

pub(super) fn with_temp_sqlite(
    bytes: &[u8],
    prefix: &str,
    parse: impl FnOnce(&Connection) -> Result<ExtractionOutcome, AnalysisServiceError>,
) -> Result<ExtractionOutcome, AnalysisServiceError> {
    let path = temp_sqlite_path(prefix);
    std::fs::write(&path, bytes)?;
    let connection = Connection::open_with_flags(
        &path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    );
    let result = match connection {
        Ok(conn) => {
            let result = parse(&conn);
            drop(conn);
            result
        }
        Err(error) => Err(error.into()),
    };
    let cleanup = std::fs::remove_file(&path);
    match (result, cleanup) {
        (Err(error), _) => Err(error),
        (Ok(outcome), Ok(())) => Ok(outcome),
        (Ok(_), Err(error)) => Err(AnalysisServiceError::Io(error)),
    }
}

fn temp_sqlite_path(prefix: &str) -> PathBuf {
    std::env::temp_dir().join(format!("forensics-{prefix}-{}.sqlite", Uuid::new_v4()))
}

pub(super) fn table_exists(db: &Connection, table: &str) -> Result<bool, AnalysisServiceError> {
    let count: i64 = db.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
        [table],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

pub(super) fn table_columns(
    db: &Connection,
    table: &str,
) -> Result<Vec<String>, AnalysisServiceError> {
    let mut stmt = db.prepare(&format!("PRAGMA table_info({})", table))?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
    let mut columns = Vec::new();
    for row in rows {
        columns.push(row?);
    }
    Ok(columns)
}
