//! Android Call Log parser (calllog.db / contacts2.db calls table).
//!
//! Parses the `calllog.db` SQLite database or the `calls` table within
//! `contacts2.db` on Android devices:
//! `/data/data/com.android.providers.contacts/databases/contacts2.db`
//! or
//! `/data/data/com.android.providers.telephony/databases/calllog.db`
//!
//! Key columns in the `calls` table:
//! - `number` — phone number
//! - `date` — call date/time (Unix milliseconds since epoch)
//! - `duration` — call duration in seconds
//! - `type` — call type (1 = incoming, 2 = outgoing, 3 = missed)
//! - `new` — whether the call is new/unread (0 or 1)
//! - `name` — cached contact name (if available)
//! - `numbertype` — type of number (home, mobile, work, etc.)
//! - `numberlabel` — custom label for the number

use chrono::TimeZone;
use serde::{Deserialize, Serialize};
use std::io::Write;

/// A parsed Android call log record.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AndroidCall {
    /// The phone number of the other party.
    pub number: Option<String>,
    /// Call duration in seconds.
    pub duration_seconds: Option<i64>,
    /// ISO 8601 timestamp of the call.
    pub date: Option<String>,
    /// Call type: 1 = incoming, 2 = outgoing, 3 = missed, 4 = voicemail, 5 = rejected, 6 = blocked.
    pub call_type: i32,
}

/// Parse a calllog database from raw bytes.
///
/// Supports both standalone `calllog.db` and the `calls` table within
/// `contacts2.db`. The parser attempts the `calls` table first; if that
/// fails, it falls back to looking for the table under a different schema.
pub fn parse_calls(data: &[u8]) -> Result<Vec<AndroidCall>, String> {
    if data.is_empty() {
        return Err("Call log data is empty".to_string());
    }

    let mut tmp = tempfile::Builder::new()
        .suffix(".calllog.db")
        .tempfile()
        .map_err(|e| format!("Failed to create temp file: {}", e))?;
    tmp.write_all(data)
        .map_err(|e| format!("Failed to write temp file: {}", e))?;
    tmp.flush()
        .map_err(|e| format!("Failed to flush temp file: {}", e))?;

    let conn = rusqlite::Connection::open(tmp.path())
        .map_err(|e| format!("Failed to open call log: {}", e))?;

    let calls = query_calls(&conn)?;
    Ok(calls)
}

fn query_calls(conn: &rusqlite::Connection) -> Result<Vec<AndroidCall>, String> {
    // The `calls` table has been observed with slightly different schemas
    // across Android versions. We try the standard columns first.
    let query = "SELECT number, duration, date, type FROM calls ORDER BY date DESC";

    let mut stmt = match conn.prepare(query) {
        Ok(s) => s,
        Err(_) => {
            // Fallback: some devices store duration as a TEXT column
            conn.prepare(
                "SELECT number, CAST(duration AS INTEGER), date, type FROM calls ORDER BY date DESC",
            )
            .map_err(|e| format!("Failed to prepare calls query: {}", e))?
        }
    };

    let calls: Vec<AndroidCall> = stmt
        .query_map([], |row| {
            let number: Option<String> = row.get(0)?;
            let duration: Option<i64> = row.get(1)?;
            let date_millis: Option<i64> = row.get(2)?;
            let call_type: i32 = row.get(3)?;

            let date = date_millis.and_then(|ms| {
                if ms <= 0 {
                    return None;
                }
                let secs = ms / 1000;
                let nsecs = ((ms % 1000) * 1_000_000) as u32;
                chrono::Utc
                    .timestamp_opt(secs, nsecs)
                    .single()
                    .map(|dt| dt.to_rfc3339())
            });

            Ok(AndroidCall {
                number,
                duration_seconds: duration,
                date,
                call_type,
            })
        })
        .map_err(|e| format!("Failed to query calls: {}", e))?
        .filter_map(|r| r.ok())
        .collect();

    Ok(calls)
}

#[cfg(test)]
#[path = "../tests/unit/calls.rs"]
mod tests;
