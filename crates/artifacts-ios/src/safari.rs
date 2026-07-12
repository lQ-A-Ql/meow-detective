//! Parse iOS Safari History database (History.db), extracting browsing history
//! records with URL, title, visit timestamps, and visit counts.
//!
//! The Safari history schema uses `history_items` for URL records and
//! `history_visits` for individual visit timestamps.

use crate::{core_data_time_to_dt, open_sqlite_from_bytes, IosArtifactError};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A parsed iOS Safari browsing history entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IosSafariEntry {
    /// The full URL visited.
    pub url: String,
    /// The page title (may be empty).
    pub title: Option<String>,
    /// Timestamp of the visit.
    pub visit_time: Option<DateTime<Utc>>,
    /// How many times this URL has been visited.
    pub visit_count: i32,
}

/// Parse an iOS Safari `History.db` and return browsing history entries.
///
/// Joins `history_items` (url, visit_count) with `history_visits` (visit_time)
/// to produce one row per visit.  Timestamps are CFAbsoluteTime (seconds since
/// 2001-01-01).
pub fn parse_safari_history(data: &[u8]) -> Result<Vec<IosSafariEntry>, IosArtifactError> {
    let (conn, _tmp) = open_sqlite_from_bytes(data)?;

    let mut stmt = conn.prepare(
        "SELECT i.url, i.title, v.visit_time, i.visit_count
         FROM history_items i
         JOIN history_visits v ON i.id = v.history_item
         ORDER BY v.visit_time DESC",
    )?;

    let rows = stmt.query_map([], |row| {
        let url: String = row.get(0)?;
        let title: Option<String> = row.get(1).ok();
        let visit_raw: f64 = row.get(2).unwrap_or(0.0);
        let visit_count: i32 = row.get(3).unwrap_or(1);
        Ok((url, title, visit_raw, visit_count))
    })?;

    let mut results = Vec::new();
    for row in rows {
        let (url, title, visit_raw, visit_count) = match row {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!("skipping safari history row: {}", e);
                continue;
            }
        };
        results.push(IosSafariEntry {
            url,
            title,
            visit_time: core_data_time_to_dt(visit_raw),
            visit_count,
        });
    }

    Ok(results)
}

#[cfg(test)]
#[path = "../tests/unit/safari.rs"]
mod tests;
