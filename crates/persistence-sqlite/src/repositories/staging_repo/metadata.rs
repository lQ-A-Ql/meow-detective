use super::StagingRepo;
use crate::DbResult;
use rusqlite::{Connection, OptionalExtension};

impl StagingRepo {
    pub fn get_staging_meta(conn: &Connection, key: &str) -> DbResult<Option<String>> {
        conn.query_row(
            "SELECT value FROM staging_meta WHERE key = ?1",
            [key],
            |row| row.get(0),
        )
        .optional()
        .map_err(Into::into)
    }
    pub fn set_staging_meta(conn: &Connection, key: &str, value: &str) -> DbResult<()> {
        conn.execute(
            "INSERT OR REPLACE INTO staging_meta(key,value) VALUES (?1,?2)",
            rusqlite::params![key, value],
        )?;
        Ok(())
    }
    pub fn get_worker_meta(conn: &Connection, key: &str) -> DbResult<Option<String>> {
        conn.query_row(
            "SELECT value FROM worker_meta WHERE key = ?1",
            [key],
            |row| row.get(0),
        )
        .optional()
        .map_err(Into::into)
    }
    pub fn set_worker_meta(conn: &Connection, key: &str, value: &str) -> DbResult<()> {
        conn.execute(
            "INSERT OR REPLACE INTO worker_meta(key,value) VALUES (?1,?2)",
            rusqlite::params![key, value],
        )?;
        Ok(())
    }
}
