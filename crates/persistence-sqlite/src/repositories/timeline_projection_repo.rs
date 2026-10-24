use crate::connection::DbResult;
use rusqlite::{params, Connection, OptionalExtension};

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
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            );",
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

    pub fn clear_file_activity_projection(&self, graph_tables_present: bool) -> DbResult<()> {
        if graph_tables_present {
            self.conn.execute_batch(
                "DELETE FROM graph_edges
                 WHERE source_id IN (
                     SELECT id FROM timeline_events
                     WHERE parser_id IN (
                        'timeline.macb', 'timeline.file_created', 'timeline.file_modified',
                        'timeline.file_accessed', 'timeline.file_deleted'
                     )
                 );
                 DELETE FROM graph_nodes
                 WHERE id IN (
                     SELECT id FROM timeline_events
                     WHERE parser_id IN (
                        'timeline.macb', 'timeline.file_created', 'timeline.file_modified',
                        'timeline.file_accessed', 'timeline.file_deleted'
                     )
                 );",
            )?;
        }
        self.conn.execute(
            "DELETE FROM timeline_events
             WHERE parser_id IN (
                'timeline.macb', 'timeline.file_created', 'timeline.file_modified',
                'timeline.file_accessed', 'timeline.file_deleted'
             )",
            [],
        )?;
        Ok(())
    }
}
