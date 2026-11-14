use rusqlite::{Connection, OpenFlags};
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DbError {
    #[error("读取操作已取消")]
    ReadCancelled,
    #[error("读取操作超过执行时间")]
    ReadTimedOut,
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Migration error: {0}")]
    Migration(String),
    #[error("{0}")]
    System(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DbErrorCategory {
    Io,
    Timeout,
    Cancelled,
    Internal,
}

impl DbError {
    pub fn category(&self) -> DbErrorCategory {
        match self {
            Self::ReadCancelled => DbErrorCategory::Cancelled,
            Self::ReadTimedOut => DbErrorCategory::Timeout,
            Self::Sqlite(_) | Self::Io(_) => DbErrorCategory::Io,
            Self::Migration(_) | Self::System(_) => DbErrorCategory::Internal,
        }
    }

    pub fn code(&self) -> Option<&'static str> {
        matches!(self, Self::Sqlite(error) if sqlite_error_is_busy(error)).then_some("DB_BUSY")
    }

    pub fn user_message(&self) -> Option<&'static str> {
        matches!(self, Self::Sqlite(error) if sqlite_error_is_busy(error))
            .then_some("数据库正在被其他任务占用，请稍后重试")
    }

    pub fn recoverable(&self) -> Option<bool> {
        matches!(self, Self::Sqlite(error) if sqlite_error_is_busy(error)).then_some(true)
    }

    pub fn suggestion(&self) -> Option<&'static str> {
        matches!(self, Self::Sqlite(error) if sqlite_error_is_busy(error))
            .then_some("请等待导入、索引或分析任务完成后重试")
    }
}

fn sqlite_error_is_busy(error: &rusqlite::Error) -> bool {
    matches!(
        error,
        rusqlite::Error::SqliteFailure(sqlite_error, _)
            if sqlite_error.extended_code == rusqlite::ffi::SQLITE_BUSY
                || sqlite_error.extended_code == rusqlite::ffi::SQLITE_LOCKED
                || sqlite_error.extended_code & 0xff == rusqlite::ffi::SQLITE_BUSY
                || sqlite_error.extended_code & 0xff == rusqlite::ffi::SQLITE_LOCKED
    )
}

pub type DbResult<T> = Result<T, DbError>;

/// Resolve the case root owning the database file behind `connection`.
///
/// `app.db` sits directly in the case root, while `source.db` lives at
/// `<case_root>/sources/<source_id>/source.db`. Any other database file has no
/// defined case-root layout and yields `None`.
pub fn case_root_from_connection(conn: &Connection) -> DbResult<Option<PathBuf>> {
    let database_path: String = conn.query_row(
        "SELECT file FROM pragma_database_list WHERE name = 'main'",
        [],
        |row| row.get(0),
    )?;
    let path = Path::new(&database_path);
    let root = match path.file_name().and_then(|name| name.to_str()) {
        Some(name) if name.eq_ignore_ascii_case("app.db") => path.parent(),
        Some(name) if name.eq_ignore_ascii_case("source.db") => {
            path.parent().and_then(Path::parent).and_then(Path::parent)
        }
        _ => None,
    };
    Ok(root.map(Path::to_path_buf))
}

pub fn open_or_create(path: &Path) -> DbResult<Connection> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let conn = Connection::open(path)?;
    conn.execute_batch(
        "PRAGMA busy_timeout=30000;
         PRAGMA journal_mode=WAL;
         PRAGMA foreign_keys=ON;
         PRAGMA synchronous=NORMAL;",
    )?;
    Ok(conn)
}

pub fn open_or_create_source(path: &Path) -> DbResult<Connection> {
    let conn = open_or_create(path)?;
    crate::migrations::runner::run_source_all(&conn)?;
    Ok(conn)
}

pub fn open_or_create_case_graph(path: &Path) -> DbResult<Connection> {
    let conn = open_or_create(path)?;
    crate::migrations::runner::run_case_graph_all(&conn)?;
    Ok(conn)
}

pub fn open_existing_case_graph_read_only(path: &Path) -> DbResult<Connection> {
    open_existing_source_read_only(path)
}

pub fn open_existing_source(path: &Path) -> DbResult<Connection> {
    // Existing source databases are opened as-is. Compatibility migrations
    // belong to explicit import/build creation paths, never to case reads.
    open_existing(path)
}

pub fn open_existing_source_read_only(path: &Path) -> DbResult<Connection> {
    crate::read_operation::ensure_active()?;
    if !path.exists() {
        return Err(DbError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("Database file not found: {}", path.display()),
        )));
    }
    let mut conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    conn.execute_batch(
        "PRAGMA query_only=ON;
         PRAGMA foreign_keys=ON;
         PRAGMA busy_timeout=30000;",
    )?;
    crate::read_operation::configure_connection(&mut conn)?;
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
        "PRAGMA busy_timeout=30000;
         PRAGMA journal_mode=WAL;
         PRAGMA foreign_keys=ON;
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
        "PRAGMA busy_timeout=30000;
         PRAGMA journal_mode=WAL;
         PRAGMA foreign_keys=ON;
         PRAGMA synchronous=NORMAL;",
    )?;
    // Run staging migration (idempotent)
    conn.execute_batch(include_str!("migrations/scripts/staging_001.sql"))?;
    Ok(conn)
}

#[cfg(test)]
#[path = "../tests/unit/connection.rs"]
mod tests;
