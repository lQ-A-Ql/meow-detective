//! Database connection management for the runtime cache.

use rusqlite::Connection;
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CacheError {
    #[error("Database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Serialization error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Not found: {0}")]
    NotFound(String),
}

pub type Result<T> = std::result::Result<T, CacheError>;

/// Open or create a runtime cache database at the given path.
///
/// Creates parent directories if they don't exist.
/// Runs all migrations on open.
pub fn open_or_create(path: &Path) -> Result<Connection> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let conn = Connection::open(path)?;
    conn.execute_batch("PRAGMA journal_mode=WAL")?;
    conn.execute_batch("PRAGMA synchronous=NORMAL")?;
    crate::migrations::run_all(&conn)?;
    Ok(conn)
}

/// Open an in-memory runtime cache database (for testing).
pub fn open_in_memory() -> Result<Connection> {
    let conn = Connection::open_in_memory()?;
    crate::migrations::run_all(&conn)?;
    Ok(conn)
}
