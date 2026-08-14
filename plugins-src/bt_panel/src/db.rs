//! Read-only in-memory SQLite access over evidence bytes.
//!
//! The host hands the plugin the whole database file in memory; the bytes
//! are transferred into SQLite-owned memory via `sqlite3_deserialize`
//! (read-only, free-on-close), so no temporary files ever touch the host
//! disk and the evidence buffer is never retained.

use rusqlite::ffi;
use rusqlite::serialize::OwnedData;
use rusqlite::{Connection, DatabaseName};
use serde_json::Value;
use std::collections::BTreeMap;
use std::ptr::NonNull;

/// A parsed panel database. All query failures are surfaced as `Err`
/// strings; the caller maps them to `MeowStatus::ParseError`.
pub struct PanelDb {
    conn: Connection,
}

impl PanelDb {
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

    pub fn table_exists(&self, table: &str) -> Result<bool, String> {
        self.conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
                [table],
                |row| row.get::<_, i64>(0),
            )
            .map(|count| count > 0)
            .map_err(|error| format!("sqlite_master query failed: {error}"))
    }

    /// All rows of `table` as column-name → JSON-value maps. Callers must
    /// whitelist fields into attrs explicitly; `SENSITIVE_COLUMNS` values
    /// may be inspected for presence but never copied out.
    pub fn rows(&self, table: &str) -> Result<Vec<BTreeMap<String, Value>>, String> {
        // `table` is always a compile-time constant from the parsers.
        let mut stmt = self
            .conn
            .prepare(&format!("SELECT * FROM \"{table}\""))
            .map_err(|error| format!("prepare on {table} failed: {error}"))?;
        let names: Vec<String> = stmt
            .column_names()
            .into_iter()
            .map(str::to_string)
            .collect();
        let mut rows = stmt
            .query([])
            .map_err(|error| format!("query on {table} failed: {error}"))?;
        let mut out = Vec::new();
        loop {
            let row = rows
                .next()
                .map_err(|error| format!("row read on {table} failed: {error}"))?;
            let Some(row) = row else { break };
            let mut map = BTreeMap::new();
            for (index, name) in names.iter().enumerate() {
                let value = row.get_ref(index).map(json_value).unwrap_or(Value::Null);
                map.insert(name.clone(), value);
            }
            out.push(map);
        }
        Ok(out)
    }
}

fn json_value(value: rusqlite::types::ValueRef<'_>) -> Value {
    match value {
        rusqlite::types::ValueRef::Null => Value::Null,
        rusqlite::types::ValueRef::Integer(number) => Value::from(number),
        rusqlite::types::ValueRef::Real(number) => Value::from(number),
        rusqlite::types::ValueRef::Text(text) => {
            Value::String(String::from_utf8_lossy(text).into_owned())
        }
        rusqlite::types::ValueRef::Blob(blob) => {
            Value::String(format!("<blob:{} bytes>", blob.len()))
        }
    }
}

/// Presence check for a sensitive column: non-empty without exposing it.
pub fn sensitive_present(row: &BTreeMap<String, Value>, column: &str) -> bool {
    match row.get(column) {
        Some(Value::String(text)) => !text.is_empty(),
        Some(Value::Null) | None => false,
        Some(_) => true,
    }
}

/// Borrow a text column; empty and NULL both read as `None`.
pub fn text<'a>(row: &'a BTreeMap<String, Value>, column: &str) -> Option<&'a str> {
    match row.get(column) {
        Some(Value::String(text)) if !text.is_empty() => Some(text.as_str()),
        _ => None,
    }
}

/// Borrow an integer column, accepting SQLite's TEXT-typed numbers too.
pub fn integer(row: &BTreeMap<String, Value>, column: &str) -> Option<i64> {
    match row.get(column) {
        Some(Value::Number(number)) => number.as_i64(),
        Some(Value::String(text)) => text.trim().parse().ok(),
        _ => None,
    }
}
