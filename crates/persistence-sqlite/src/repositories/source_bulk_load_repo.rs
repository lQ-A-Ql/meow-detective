use crate::connection::DbResult;
use rusqlite::{Connection, OptionalExtension};

pub const BULK_LOAD_PREFIX: &str = "bulk_load.";
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BulkLoadState {
    Inactive,
    Active,
    Rebuilding,
    Ready,
}
impl BulkLoadState {
    fn as_str(self) -> &'static str {
        match self {
            Self::Inactive => "inactive",
            Self::Active => "active",
            Self::Rebuilding => "rebuilding",
            Self::Ready => "ready",
        }
    }
}
pub struct SourceBulkLoadRepo;
impl SourceBulkLoadRepo {
    pub fn set_state(conn: &Connection, state: BulkLoadState, attempt: &str) -> DbResult<()> {
        let tx = conn.unchecked_transaction()?;
        tx.execute(
            "INSERT OR REPLACE INTO source_meta(key,value) VALUES(?1,?2)",
            rusqlite::params![format!("{BULK_LOAD_PREFIX}state"), state.as_str()],
        )?;
        tx.execute(
            "INSERT OR REPLACE INTO source_meta(key,value) VALUES(?1,?2)",
            rusqlite::params![format!("{BULK_LOAD_PREFIX}attempt"), attempt],
        )?;
        tx.commit()?;
        Ok(())
    }
    pub fn state(conn: &Connection) -> DbResult<BulkLoadState> {
        let value: Option<String> = conn
            .query_row(
                "SELECT value FROM source_meta WHERE key=?1",
                [format!("{BULK_LOAD_PREFIX}state")],
                |row| row.get(0),
            )
            .optional()?;
        let state = match value.as_deref() {
            Some("active") => BulkLoadState::Active,
            Some("rebuilding") => BulkLoadState::Rebuilding,
            Some("ready") => BulkLoadState::Ready,
            None | Some("inactive") => BulkLoadState::Inactive,
            _ => {
                return Err(crate::DbError::System(
                    "Unknown source bulk-load state".into(),
                ))
            }
        };
        if state == BulkLoadState::Inactive {
            let orphaned: bool = conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM source_meta WHERE key='bulk_load.indexes')",
                [],
                |r| r.get(0),
            )?;
            if orphaned {
                return Err(crate::DbError::System(
                    "Orphaned source index manifest requires import recovery".into(),
                ));
            }
        }
        Ok(state)
    }
    pub fn clear_transient(conn: &Connection) -> DbResult<()> {
        conn.execute(
            "DELETE FROM source_meta WHERE key GLOB ?1",
            [format!("{BULK_LOAD_PREFIX}*")],
        )?;
        Ok(())
    }
}
