//! Database migrations for the runtime cache.

use rusqlite::Connection;

/// Run all migrations on the given connection.
pub fn run_all(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS cache_entries (
            cache_key TEXT PRIMARY KEY,
            namespace TEXT NOT NULL,
            case_id TEXT,
            value_json TEXT NOT NULL,
            created_at TEXT NOT NULL,
            expires_at TEXT,
            last_accessed_at TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_cache_namespace ON cache_entries(namespace);
        CREATE INDEX IF NOT EXISTS idx_cache_expires ON cache_entries(expires_at);
        CREATE INDEX IF NOT EXISTS idx_cache_case ON cache_entries(case_id);

        CREATE TABLE IF NOT EXISTS file_handles (
            handle_id TEXT PRIMARY KEY,
            case_id TEXT NOT NULL,
            object_id TEXT NOT NULL,
            opened_at TEXT NOT NULL,
            expires_at TEXT NOT NULL,
            access_mode TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_handles_case ON file_handles(case_id);
        CREATE INDEX IF NOT EXISTS idx_handles_expires ON file_handles(expires_at);
        ",
    )?;
    Ok(())
}
