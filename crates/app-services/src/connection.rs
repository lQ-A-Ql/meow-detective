//! Case-scoped database connection helpers.
//!
//! This module centralizes opening SQLite case databases so that the Tauri
//! command layer does not directly depend on `persistence_sqlite` for
//! connection management.

use std::path::Path;

use rusqlite::Connection;

/// Open (or create) the SQLite database at `path` and run all migrations.
///
/// This is a thin wrapper around `persistence_sqlite::open_or_create` so the
/// command layer can delegate connection creation through `app-services`.
pub fn open_case_db(path: &Path) -> Result<Connection, persistence_sqlite::DbError> {
    let conn = persistence_sqlite::open_or_create(path)?;
    persistence_sqlite::runner::run_all(&conn)?;
    Ok(conn)
}

/// Open an already initialized case database without rerunning migrations.
///
/// Background workers use this path after the parent job has initialized the
/// case. Avoiding repeated migration checks keeps concurrent cluster members
/// out of the schema runner and leaves SQLite writes to the normal WAL path.
pub fn open_existing_case_db(path: &Path) -> Result<Connection, persistence_sqlite::DbError> {
    persistence_sqlite::connection::open_existing(path)
}
