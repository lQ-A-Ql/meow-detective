//! Read-only in-memory SQLite access over evidence bytes.
//!
//! The host hands the plugin the whole database file in memory; the bytes
//! are transferred into SQLite-owned memory via `sqlite3_deserialize`
//! (read-only, free-on-close), so no temporary files ever touch the host
//! disk and the evidence buffer is never retained. Only used for plaintext
//! databases — WCDB/SQLCipher-encrypted files never reach this module.
//!
//! WAL-mode normalization: WeChat 4.x (WCDB) databases carry read/write
//! version 2 (WAL) in the header, and `sqlite3_deserialize` rejects
//! WAL-mode images outright (`SQLITE_CANTOPEN`). The host supplies a sibling
//! `-wal` through the ABI companion list when present; `walmerge` validates
//! and applies committed frames first. The resulting private copy's version
//! bytes are then downgraded to 1 (rollback) before deserialization.

use rusqlite::ffi;
use rusqlite::serialize::OwnedData;
use rusqlite::{Connection, DatabaseName};
use std::ptr::NonNull;

/// A parsed plaintext WeChat database. All query failures are surfaced as
/// `Err` strings; the caller maps them to `MeowStatus::ParseError`.
pub struct WeChatDb {
    conn: Connection,
}

impl WeChatDb {
    pub fn from_bytes(data: &[u8]) -> Result<Self, String> {
        if data.is_empty() {
            return Err("empty database input".to_string());
        }
        let mut conn = Connection::open_in_memory()
            .map_err(|error| format!("sqlite open_in_memory failed: {error}"))?;
        // SAFETY: `sqlite3_malloc` allocates SQLite-owned memory that
        // `OwnedData` frees with `sqlite3_free` on drop (or on close once
        // deserialized with FREEONCLOSE); `data` is fully copied in.
        let owned = unsafe {
            let raw = ffi::sqlite3_malloc(data.len() as i32);
            if raw.is_null() {
                return Err("sqlite3_malloc failed for input buffer".to_string());
            }
            std::ptr::copy_nonoverlapping(data.as_ptr(), raw.cast::<u8>(), data.len());
            // WAL downgrade on the private copy (see module docs): bytes
            // 18/19 are the file-format read/write versions.
            if data.len() >= 20 && &data[..16] == b"SQLite format 3\0" && data[18] == 2 {
                raw.cast::<u8>().add(18).write(1);
                raw.cast::<u8>().add(19).write(1);
            }
            OwnedData::from_raw_nonnull(
                NonNull::new(raw.cast::<u8>()).expect("non-null sqlite allocation"),
                data.len(),
            )
        };
        conn.deserialize(DatabaseName::Main, owned, true)
            .map_err(|error| format!("sqlite deserialize failed: {error}"))?;
        // `sqlite3_deserialize` does not validate up front; force a real
        // read so corrupt/truncated input fails here as ParseError.
        conn.query_row("SELECT count(*) FROM sqlite_master", [], |row| {
            row.get::<_, i64>(0)
        })
        .map_err(|error| format!("not a readable SQLite database: {error}"))?;
        Ok(Self { conn })
    }

    /// User table names (SQLite-internal `sqlite_*` tables excluded),
    /// ordered for deterministic output.
    pub fn table_list(&self) -> Result<Vec<String>, String> {
        let mut stmt = self
            .conn
            .prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name")
            .map_err(|error| format!("sqlite_master prepare failed: {error}"))?;
        let names = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|error| format!("sqlite_master query failed: {error}"))?;
        let mut out = Vec::new();
        for name in names {
            out.push(name.map_err(|error| format!("sqlite_master row failed: {error}"))?);
        }
        Ok(out)
    }

    /// Row count of a table discovered through `table_list` (never raw
    /// request input; the identifier is double-quote escaped anyway).
    pub fn row_count(&self, table: &str) -> Result<i64, String> {
        let escaped = table.replace('"', "\"\"");
        self.conn
            .query_row(&format!("SELECT count(*) FROM \"{escaped}\""), [], |row| {
                row.get::<_, i64>(0)
            })
            .map_err(|error| format!("row count on {table} failed: {error}"))
    }

    /// Whether a user table exists (exact name match in `sqlite_master`).
    pub fn table_exists(&self, table: &str) -> Result<bool, String> {
        self.conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
                [table],
                |row| row.get::<_, i64>(0),
            )
            .map(|count| count > 0)
            .map_err(|error| format!("table existence check on {table} failed: {error}"))
    }

    pub fn column_exists(&self, table: &str, column: &str) -> Result<bool, String> {
        let escaped = table.replace('"', "\"\"");
        let mut statement = self
            .conn
            .prepare(&format!("PRAGMA table_info(\"{escaped}\")"))
            .map_err(|error| format!("table_info on {table} failed: {error}"))?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(1))
            .map_err(|error| format!("table_info query on {table} failed: {error}"))?;
        for row in rows {
            if row
                .map_err(|error| format!("table_info row on {table} failed: {error}"))?
                .eq_ignore_ascii_case(column)
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Raw connection for the content parsers. Callers only run SELECTs
    /// against the in-memory deserialized copy; the evidence bytes are
    /// read-only by construction.
    pub fn conn(&self) -> &Connection {
        &self.conn
    }
}
