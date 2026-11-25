use crate::connection::DbResult;
use rusqlite::{params, Connection, OptionalExtension};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimelineProjectionMetaState {
    Done,
    Deferred,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimelineProjectionMetaStatus {
    pub state: TimelineProjectionMetaState,
    pub inserted_count: u64,
    pub input_identity: String,
    pub required_bytes: u64,
    pub available_bytes: u64,
    pub deferred_reason: Option<String>,
    pub updated_at: String,
}

pub struct TimelineProjectionRepo<'a> {
    conn: &'a Connection,
}

impl<'a> TimelineProjectionRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn ensure_meta_table(&self) -> DbResult<()> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS timeline_projection_meta (
                projection_key TEXT PRIMARY KEY NOT NULL,
                status TEXT NOT NULL,
                inserted_count INTEGER NOT NULL DEFAULT 0,
                input_identity TEXT NOT NULL,
                deferred_reason TEXT,
                required_bytes INTEGER NOT NULL DEFAULT 0,
                available_bytes INTEGER NOT NULL DEFAULT 0,
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            );",
        )?;
        self.conn.prepare(
            "SELECT projection_key,status,inserted_count,input_identity,deferred_reason,
             required_bytes,available_bytes,updated_at FROM timeline_projection_meta LIMIT 0",
        )?;
        Ok(())
    }

    pub fn is_done(&self, key: &str, input_identity: &str) -> DbResult<bool> {
        let status = self
            .conn
            .query_row(
                "SELECT status, input_identity FROM timeline_projection_meta WHERE projection_key = ?1",
                [key],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()?;
        Ok(status.is_some_and(|(status, identity)| status == "done" && identity == input_identity))
    }

    pub fn mark_done(&self, key: &str, inserted_count: u64, input_identity: &str) -> DbResult<()> {
        self.conn.execute(
            "INSERT INTO timeline_projection_meta
             (projection_key, status, inserted_count, input_identity, updated_at)
             VALUES (?1, 'done', ?2, ?3, datetime('now'))
             ON CONFLICT(projection_key) DO UPDATE SET
                status = excluded.status,
                inserted_count = excluded.inserted_count,
                input_identity = excluded.input_identity,
                updated_at = excluded.updated_at",
            params![key, inserted_count as i64, input_identity],
        )?;
        Ok(())
    }

    pub fn mark_deferred(
        &self,
        key: &str,
        input_identity: &str,
        required_bytes: u64,
        available_bytes: u64,
        reason: &str,
    ) -> DbResult<()> {
        self.conn.execute(
            "INSERT INTO timeline_projection_meta
             (projection_key, status, inserted_count, input_identity, deferred_reason, required_bytes, available_bytes, updated_at)
             VALUES (?1, 'deferred', 0, ?2, ?3, ?4, ?5, datetime('now'))
             ON CONFLICT(projection_key) DO UPDATE SET
                status = 'deferred',
                inserted_count = 0,
                input_identity = excluded.input_identity,
                deferred_reason = excluded.deferred_reason,
                required_bytes = excluded.required_bytes,
                available_bytes = excluded.available_bytes,
                updated_at = excluded.updated_at",
            params![key, input_identity, reason, required_bytes as i64, available_bytes as i64],
        )?;
        Ok(())
    }

    pub fn status(&self, key: &str) -> DbResult<Option<TimelineProjectionMetaStatus>> {
        let row = self
            .conn
            .query_row(
                "SELECT status, inserted_count, input_identity, required_bytes, available_bytes, deferred_reason, updated_at
                 FROM timeline_projection_meta WHERE projection_key = ?1",
                [key],
                |row| {
                    let status: String = row.get(0)?;
                    let state = match status.as_str() {
                        "done" => TimelineProjectionMetaState::Done,
                        _ => TimelineProjectionMetaState::Deferred,
                    };
                    Ok(TimelineProjectionMetaStatus {
                        state,
                        inserted_count: row.get::<_, i64>(1)?.max(0) as u64,
                        input_identity: row.get(2)?,
                        required_bytes: row.get::<_, i64>(3)?.max(0) as u64,
                        available_bytes: row.get::<_, i64>(4)?.max(0) as u64,
                        deferred_reason: row.get(5)?,
                        updated_at: row.get(6)?,
                    })
                },
            )
            .optional()?;
        Ok(row)
    }

    pub fn clear_file_activity_projection(&self, graph_tables_present: bool) -> DbResult<()> {
        let _ = graph_tables_present;
        Err(crate::connection::DbError::System(
            "timeline detail projection is owned by Fjall".into(),
        ))
    }
}
