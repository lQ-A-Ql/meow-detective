use rusqlite::Connection;

use crate::connection::DbResult;

pub fn run_all(conn: &Connection) -> DbResult<u32> {
    super::runner::run_migrations(conn, super::runner::MIGRATIONS)
}
