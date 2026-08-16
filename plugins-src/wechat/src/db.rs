//! Read-only in-memory SQLite access over evidence bytes.
//!
//! The host hands the plugin the whole database file in memory; the bytes
//! are transferred into SQLite-owned memory via `sqlite3_deserialize`
//! (read-only, free-on-close), so no temporary files ever touch the host
//! disk and the evidence buffer is never retained. Only used for plaintext
//! databases — WCDB/SQLCipher-encrypted files never reach this module.

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
}
