use crate::connection::DbResult;
use rusqlite::Connection;

const MIGRATIONS: &[(&str, &str)] =
    &[("case_graph_001", include_str!("scripts/case_graph_001.sql"))];

pub fn latest_version() -> &'static str {
    MIGRATIONS
        .last()
        .map(|(name, _)| *name)
        .expect("case graph migration registry must not be empty")
}

pub fn run_all(conn: &Connection) -> DbResult<u32> {
    super::runner::run_migrations(conn, MIGRATIONS)
}
