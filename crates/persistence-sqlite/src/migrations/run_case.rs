use rusqlite::Connection;

use crate::connection::DbResult;

pub fn run_all(conn: &Connection) -> DbResult<u32> {
    super::runner::run_migrations(conn, super::runner::MIGRATIONS)
}

pub fn current_version(conn: &Connection) -> DbResult<Option<String>> {
    super::version::current_version(
        conn,
        super::runner::MIGRATIONS,
        super::source_registry::SOURCE_MIGRATIONS,
    )
}
