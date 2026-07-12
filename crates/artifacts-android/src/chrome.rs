//! Android Chrome History parser.
//!
//! Parses the Chrome/Chromium `History` SQLite database found on Android:
//! `/data/data/com.android.chrome/app_chrome/Default/History`
//!
//! Key tables:
//! - `urls` — URL entries (id, url, title, visit_count, last_visit_time)
//! - `visits` — visit records (id, url, visit_time, from_visit, transition, etc.)
//!
//! Chrome stores timestamps as microseconds since 1601-01-01 (Windows NT epoch),
//! same as Chromium on desktop. This is known as the "WebKit" or "Chrome" epoch.

use chrono::TimeZone;
use serde::{Deserialize, Serialize};
use std::io::Write;

/// A parsed Android Chrome browsing history visit.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AndroidChromeVisit {
    pub url: String,
    pub title: Option<String>,
    /// ISO 8601 timestamp of the visit.
    pub visit_time: Option<String>,
}

/// Chrome timestamps are in microseconds since 1601-01-01T00:00:00Z (Windows NT epoch).
/// The Unix epoch is 11644473600 seconds after the NT epoch.
const NT_EPOCH_OFFSET_MICROS: i64 = 11_644_473_600_000_000;

/// Parse an Android Chrome History database from raw bytes.
pub fn parse_chrome_history(data: &[u8]) -> Result<Vec<AndroidChromeVisit>, String> {
    if data.is_empty() {
        return Err("Chrome History data is empty".to_string());
    }

    let mut tmp = tempfile::Builder::new()
        .suffix(".chrome.db")
        .tempfile()
        .map_err(|e| format!("Failed to create temp file: {}", e))?;
    tmp.write_all(data)
        .map_err(|e| format!("Failed to write temp file: {}", e))?;
    tmp.flush()
        .map_err(|e| format!("Failed to flush temp file: {}", e))?;

    let conn = rusqlite::Connection::open(tmp.path())
        .map_err(|e| format!("Failed to open Chrome History: {}", e))?;

    let visits = query_chrome_history(&conn)?;
    Ok(visits)
}

fn query_chrome_history(conn: &rusqlite::Connection) -> Result<Vec<AndroidChromeVisit>, String> {
    // Join urls and visits to get URL + title + visit_time
    let query = "SELECT u.url, u.title, v.visit_time
                 FROM urls u
                 JOIN visits v ON u.id = v.url
                 ORDER BY v.visit_time DESC
                 LIMIT 10000";

    let mut stmt = conn
        .prepare(query)
        .map_err(|e| format!("Failed to prepare Chrome history query: {}", e))?;

    let visits: Vec<AndroidChromeVisit> = stmt
        .query_map([], |row| {
            let url: String = row.get(0)?;
            let title: Option<String> = row.get(1)?;
            let visit_time_micros: i64 = row.get(2)?;

            let visit_time = convert_chrome_timestamp(visit_time_micros);

            Ok(AndroidChromeVisit {
                url,
                title,
                visit_time,
            })
        })
        .map_err(|e| format!("Failed to query Chrome history: {}", e))?
        .filter_map(|r| r.ok())
        .collect();

    Ok(visits)
}

/// Convert a Chrome microsecond timestamp to ISO 8601.
fn convert_chrome_timestamp(micros: i64) -> Option<String> {
    if micros <= 0 {
        return None;
    }

    // Chrome epoch is 1601-01-01. To get Unix microseconds, subtract the offset.
    let unix_micros = micros - NT_EPOCH_OFFSET_MICROS;
    if unix_micros < 0 {
        return None;
    }
    let secs = unix_micros / 1_000_000;
    let nsecs = ((unix_micros % 1_000_000) * 1000) as u32;

    chrono::Utc
        .timestamp_opt(secs, nsecs)
        .single()
        .map(|dt| dt.to_rfc3339())
}

#[cfg(test)]
#[path = "../tests/unit/chrome.rs"]
mod tests;
