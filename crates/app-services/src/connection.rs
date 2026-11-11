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

struct DbErrorCommandAdapter(persistence_sqlite::DbError);

impl std::fmt::Display for DbErrorCommandAdapter {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.0, formatter)
    }
}

impl transport::ServiceErrorCategory for DbErrorCommandAdapter {
    fn category(&self) -> transport::ErrorCategory {
        match self.0.category() {
            persistence_sqlite::DbErrorCategory::Cancelled => transport::ErrorCategory::Cancelled,
            persistence_sqlite::DbErrorCategory::Timeout => transport::ErrorCategory::Timeout,
            persistence_sqlite::DbErrorCategory::Io => transport::ErrorCategory::Io,
            persistence_sqlite::DbErrorCategory::Internal => transport::ErrorCategory::Internal,
        }
    }

    fn code(&self) -> Option<&'static str> {
        self.0.code()
    }

    fn user_message(&self) -> Option<&'static str> {
        self.0.user_message()
    }

    fn recoverable(&self) -> Option<bool> {
        self.0.recoverable()
    }

    fn suggestion(&self) -> Option<&'static str> {
        self.0.suggestion()
    }
}

/// Map a case-store database error into the command contract without making
/// `persistence-sqlite` depend on `transport`.
pub fn command_error_from_db(error: persistence_sqlite::DbError) -> transport::CommandError {
    transport::CommandError::from_typed_service_error(DbErrorCommandAdapter(error))
}
