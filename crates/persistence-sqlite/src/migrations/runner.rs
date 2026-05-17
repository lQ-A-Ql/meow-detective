use crate::connection::{DbError, DbResult};
use rusqlite::Connection;

const MIGRATIONS: &[(&str, &str)] = &[
    ("0001_cases", include_str!("scripts/0001_cases.sql")),
    (
        "0002_data_sources",
        include_str!("scripts/0002_data_sources.sql"),
    ),
    (
        "0003_file_entries",
        include_str!("scripts/0003_file_entries.sql"),
    ),
    ("0004_artifacts", include_str!("scripts/0004_artifacts.sql")),
    (
        "0005_timeline_events",
        include_str!("scripts/0005_timeline_events.sql"),
    ),
    ("0006_jobs", include_str!("scripts/0006_jobs.sql")),
    ("0007_reports", include_str!("scripts/0007_reports.sql")),
    ("0008_tags", include_str!("scripts/0008_tags.sql")),
];

pub fn run_all(conn: &Connection) -> DbResult<u32> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL UNIQUE,
            applied_at TEXT NOT NULL DEFAULT (datetime('now'))
        );",
    )?;

    let mut count = 0u32;
    for (name, sql) in MIGRATIONS {
        let already_applied: bool = match conn.query_row(
            "SELECT COUNT(*) > 0 FROM schema_migrations WHERE name = ?1",
            [name],
            |row| row.get(0),
        ) {
            Ok(v) => v,
            Err(rusqlite::Error::QueryReturnedNoRows) => false,
            Err(e) => return Err(e.into()),
        };

        if !already_applied {
            conn.execute_batch(sql)
                .map_err(|e| DbError::Migration(format!("Failed to apply {}: {}", name, e)))?;
            conn.execute("INSERT INTO schema_migrations (name) VALUES (?1)", [name])?;
            count += 1;
        }
    }
    Ok(count)
}

pub fn current_version(conn: &Connection) -> DbResult<Option<String>> {
    let has_table: bool = conn
        .query_row(
            "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='schema_migrations'",
            [],
            |row| row.get(0),
        )
        .unwrap_or(false);

    if !has_table {
        return Ok(None);
    }

    let result = conn.query_row(
        "SELECT name FROM schema_migrations ORDER BY id DESC LIMIT 1",
        [],
        |row| row.get::<_, String>(0),
    );
    match result {
        Ok(name) => Ok(Some(name)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}
