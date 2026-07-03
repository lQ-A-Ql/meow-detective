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
