//! Durable readiness target for completing an interrupted source import.

use persistence_sqlite::{DbError, DbResult};
use rusqlite::Connection;

const IMPORT_READY_STATE_KEY: &str = "import_ready_state";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImportedSourceState {
    Ready,
    ReadyMetadata,
}

impl ImportedSourceState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::ReadyMetadata => "ready_metadata",
        }
    }
}

pub fn record_import_target(conn: &Connection, target: ImportedSourceState) -> DbResult<()> {
    conn.execute(
        "INSERT INTO source_meta(key,value) VALUES (?1,?2)
         ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        rusqlite::params![IMPORT_READY_STATE_KEY, target.as_str()],
    )?;
    Ok(())
}

pub fn read_import_target(conn: &Connection) -> DbResult<ImportedSourceState> {
    let value: String = conn.query_row(
        "SELECT value FROM source_meta WHERE key=?1",
        [IMPORT_READY_STATE_KEY],
        |row| row.get(0),
    )?;
    match value.as_str() {
        "ready" => Ok(ImportedSourceState::Ready),
        "ready_metadata" => Ok(ImportedSourceState::ReadyMetadata),
        _ => Err(DbError::System(
            "source import readiness target is invalid".into(),
        )),
    }
}
