use rusqlite::{Connection, OpenFlags};
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DbError {
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Migration error: {0}")]
    Migration(String),
    #[error("{0}")]
    System(String),
}

impl transport::ServiceErrorCategory for DbError {
    fn category(&self) -> transport::ErrorCategory {
        match self {
            Self::Sqlite(_) | Self::Io(_) => transport::ErrorCategory::Io,
            Self::Migration(_) | Self::System(_) => transport::ErrorCategory::Internal,
        }
    }
}

pub type DbResult<T> = Result<T, DbError>;

pub fn open_or_create(path: &Path) -> DbResult<Connection> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let conn = Connection::open(path)?;
    conn.execute_batch(
        "PRAGMA journal_mode=WAL;
         PRAGMA foreign_keys=ON;
         PRAGMA busy_timeout=5000;
         PRAGMA synchronous=NORMAL;",
    )?;
    Ok(conn)
}

pub fn open_or_create_source(path: &Path) -> DbResult<Connection> {
    let conn = open_or_create(path)?;
    crate::migrations::runner::run_source_all(&conn)?;
    Ok(conn)
}

pub fn open_existing_source(path: &Path) -> DbResult<Connection> {
    let conn = open_existing(path)?;
    crate::migrations::runner::run_source_all(&conn)?;
    Ok(conn)
}

/// Open an existing database file. Returns an error if the file does not exist.
pub fn open_existing(path: &Path) -> DbResult<Connection> {
    if !path.exists() {
        return Err(DbError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("Database file not found: {}", path.display()),
        )));
    }
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_WRITE)?;
    conn.execute_batch(
        "PRAGMA journal_mode=WAL;
         PRAGMA foreign_keys=ON;
         PRAGMA busy_timeout=5000;
         PRAGMA synchronous=NORMAL;",
    )?;
    Ok(conn)
}

pub fn open_in_memory() -> DbResult<Connection> {
    let conn = Connection::open_in_memory()?;
    conn.execute_batch("PRAGMA foreign_keys=ON;")?;
    Ok(conn)
}

/// Open or create a staging database for parallel import.
/// Runs the staging-specific migration (file_entries + staging_meta).
pub fn open_staging(path: &Path) -> DbResult<Connection> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let conn = Connection::open(path)?;
    conn.execute_batch(
        "PRAGMA journal_mode=WAL;
         PRAGMA foreign_keys=ON;
         PRAGMA busy_timeout=5000;
         PRAGMA synchronous=NORMAL;",
    )?;
    // Run staging migration (idempotent)
    conn.execute_batch(include_str!("migrations/scripts/staging_001.sql"))?;
    Ok(conn)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_in_memory_can_query() {
        let conn = open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE test_tbl (id INTEGER PRIMARY KEY, val TEXT)")
            .unwrap();
        conn.execute("INSERT INTO test_tbl (val) VALUES ('hello')", [])
            .unwrap();
        let val: String = conn
            .query_row("SELECT val FROM test_tbl WHERE id = 1", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(val, "hello");
    }

    #[test]
    fn open_in_memory_foreign_keys_enabled() {
        let conn = open_in_memory().unwrap();
        let fk: i32 = conn
            .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
            .unwrap();
        assert_eq!(fk, 1);
    }

    #[test]
    fn open_staging_creates_meta_table() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("test_staging.db");
        let conn = open_staging(&path).unwrap();

        // staging_meta table should exist
        let tbl: String = conn
            .query_row(
                "SELECT name FROM sqlite_master WHERE type='table' AND name='staging_meta'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(tbl, "staging_meta");

        // file_entries table should exist
        let tbl: String = conn
            .query_row(
                "SELECT name FROM sqlite_master WHERE type='table' AND name='file_entries'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(tbl, "file_entries");
    }

    #[test]
    fn open_staging_is_idempotent() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("test_staging.db");
        let conn1 = open_staging(&path).unwrap();
        conn1
            .execute(
                "INSERT INTO staging_meta (key, value) VALUES ('k', 'v')",
                [],
            )
            .unwrap();
        drop(conn1);

        // Opening again should not fail or lose data
        let conn2 = open_staging(&path).unwrap();
        let val: String = conn2
            .query_row(
                "SELECT value FROM staging_meta WHERE key = 'k'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(val, "v");
    }

    #[test]
    fn open_or_create_source_runs_source_schema() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("sources").join("ds-1").join("source.db");
        let conn = open_or_create_source(&path).unwrap();

        for table in [
            "source_meta",
            "data_sources",
            "data_source_partitions",
            "file_entries",
            "artifacts",
            "timeline_events",
        ] {
            let found: String = conn
                .query_row(
                    "SELECT name FROM sqlite_master WHERE type='table' AND name = ?1",
                    [table],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(found, table);
        }
    }

    #[test]
    fn open_or_create_creates_file() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("subdir").join("test.db");
        assert!(!path.exists());

        let conn = open_or_create(&path).unwrap();
        conn.execute_batch("CREATE TABLE t (id INTEGER)").unwrap();
        assert!(path.exists());
    }

    #[test]
    fn open_or_create_wal_mode() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("wal_test.db");
        let conn = open_or_create(&path).unwrap();
        let journal: String = conn
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .unwrap();
        assert_eq!(journal, "wal");
    }

    #[test]
    fn open_existing_fails_if_missing() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("nonexistent.db");
        let result = open_existing(&path);
        assert!(result.is_err());
    }

    #[test]
    fn open_existing_works_on_existing_file() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("existing.db");

        // Create the file first
        let conn_create = open_or_create(&path).unwrap();
        conn_create
            .execute_batch("CREATE TABLE t (id INTEGER)")
            .unwrap();
        drop(conn_create);

        // Now open_existing should work
        let conn = open_existing(&path).unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM t", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }
}
