//! Repository for file handle cache entries.

use crate::connection::{CacheError, Result};
use crate::models::FileHandleCache;
use chrono::{DateTime, Duration, Utc};
use rusqlite::{params, Connection};
use uuid::Uuid;

/// Repository for managing file handle cache entries.
pub struct HandleRepo<'a> {
    conn: &'a Connection,
}

impl<'a> HandleRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// Get a file handle by ID.
    ///
    /// Returns None if not found or expired.
    pub fn get(&self, handle_id: &str) -> Result<Option<FileHandleCache>> {
        let mut stmt = self.conn.prepare(
            "SELECT handle_id, case_id, object_id, opened_at, expires_at, access_mode
             FROM file_handles
             WHERE handle_id = ?1 AND expires_at > ?2",
        )?;

        let now = Utc::now().to_rfc3339();
        let result = stmt.query_row(params![handle_id, now], |row| {
            Ok(FileHandleCache {
                handle_id: row.get(0)?,
                case_id: row.get(1)?,
                object_id: row.get(2)?,
                opened_at: parse_datetime(&row.get::<_, String>(3)?),
                expires_at: parse_datetime(&row.get::<_, String>(4)?),
                access_mode: row.get(5)?,
            })
        });

        match result {
            Ok(handle) => Ok(Some(handle)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(CacheError::Sqlite(e)),
        }
    }

    /// Create a new file handle.
    ///
    /// Returns the generated handle ID.
    pub fn create(&self, case_id: &str, object_id: &str, ttl: Duration) -> Result<String> {
        let handle_id = Uuid::new_v4().to_string();
        let now = Utc::now();
        let expires_at = now + ttl;

        self.conn.execute(
            "INSERT INTO file_handles (handle_id, case_id, object_id, opened_at, expires_at, access_mode)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                handle_id,
                case_id,
                object_id,
                now.to_rfc3339(),
                expires_at.to_rfc3339(),
                "read",
            ],
        )?;

        Ok(handle_id)
    }

    /// Delete a file handle by ID.
    pub fn delete(&self, handle_id: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM file_handles WHERE handle_id = ?1",
            params![handle_id],
        )?;
        Ok(())
    }

    /// Delete all expired handles.
    ///
    /// Returns the number of deleted handles.
    pub fn cleanup_expired(&self) -> Result<u64> {
        let now = Utc::now().to_rfc3339();
        let count = self.conn.execute(
            "DELETE FROM file_handles WHERE expires_at <= ?1",
            params![now],
        )?;
        Ok(count as u64)
    }

    /// Delete all handles for a case.
    ///
    /// Returns the number of deleted handles.
    pub fn clear_case(&self, case_id: &str) -> Result<u64> {
        let count = self.conn.execute(
            "DELETE FROM file_handles WHERE case_id = ?1",
            params![case_id],
        )?;
        Ok(count as u64)
    }

    /// Count handles for a case.
    pub fn count_case(&self, case_id: &str) -> Result<u64> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM file_handles WHERE case_id = ?1",
            params![case_id],
            |r| r.get(0),
        )?;
        Ok(count as u64)
    }
}

fn parse_datetime(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now())
}

#[cfg(test)]
#[path = "../../tests/unit/repositories/handle_repo.rs"]
mod tests;
