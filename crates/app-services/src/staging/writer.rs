use persistence_sqlite::repositories::staging_repo::StagingRepo;
use persistence_sqlite::DbResult;
use rusqlite::Connection;

/// Writer-facing staging operations.
///
/// Import orchestration owns one connection per staging database. These helpers
/// deliberately operate on a caller-owned connection so they cannot create
/// hidden competing writers.
pub fn staging_db_row_count(conn: &Connection) -> DbResult<u64> {
    StagingRepo::staging_db_row_count(conn)
}

pub fn analysis_staging_counts(conn: &Connection) -> DbResult<(u64, u64, u64)> {
    StagingRepo::analysis_staging_counts(conn)
}

pub fn get_staging_meta(conn: &Connection, key: &str) -> DbResult<Option<String>> {
    StagingRepo::get_staging_meta(conn, key)
}

pub fn set_staging_meta(conn: &Connection, key: &str, value: &str) -> DbResult<()> {
    StagingRepo::set_staging_meta(conn, key, value)
}

pub fn get_worker_meta(conn: &Connection, key: &str) -> DbResult<Option<String>> {
    StagingRepo::get_worker_meta(conn, key)
}

pub fn set_worker_meta(conn: &Connection, key: &str, value: &str) -> DbResult<()> {
    StagingRepo::set_worker_meta(conn, key, value)
}
