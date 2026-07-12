//! Parse iOS Notes database (Notes.sqlite), extracting note records with
//! title, snippet, and timestamps.
//!
//! The Notes app stores data in CoreData tables: `ZNOTE` holds the note body
//! (as `ZSNIPPET` or `ZHTMLSTRING`) and metadata; `ZNOTEBODY` may hold the
//! full content in some schema versions.

use crate::{core_data_time_to_dt, open_sqlite_from_bytes, IosArtifactError};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A parsed iOS Notes entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IosNote {
    /// Note title (first line of the note, or explicit title).
    pub title: Option<String>,
    /// Short text preview / snippet of the note content.
    pub snippet: Option<String>,
    /// Creation timestamp.
    pub created_at: Option<DateTime<Utc>>,
    /// Last modification timestamp.
    pub modified_at: Option<DateTime<Utc>>,
}

/// Parse an iOS `Notes.sqlite` and return extracted notes.
///
/// Queries `ZNOTE` for title, snippet, and timestamps.  Falls back to
/// `ZNOTEBODY` for full content if available.
pub fn parse_notes(data: &[u8]) -> Result<Vec<IosNote>, IosArtifactError> {
    let (conn, _tmp) = open_sqlite_from_bytes(data)?;

    let mut stmt = conn.prepare(
        "SELECT ZTITLE, ZSNIPPET, ZCREATIONDATE, ZMODIFICATIONDATE
         FROM ZNOTE
         ORDER BY ZMODIFICATIONDATE DESC",
    )?;

    let rows = stmt.query_map([], |row| {
        let title: Option<String> = row.get(0).ok();
        let snippet: Option<String> = row.get(1).ok();
        let created_raw: Option<f64> = row.get(2).ok();
        let modified_raw: Option<f64> = row.get(3).ok();
        Ok((title, snippet, created_raw, modified_raw))
    })?;

    let mut results = Vec::new();
    for row in rows {
        let (title, snippet, created_raw, modified_raw) = match row {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!("skipping notes row: {}", e);
                continue;
            }
        };

        results.push(IosNote {
            title,
            snippet,
            created_at: created_raw.and_then(core_data_time_to_dt),
            modified_at: modified_raw.and_then(core_data_time_to_dt),
        });
    }

    Ok(results)
}

#[cfg(test)]
#[path = "../tests/unit/notes.rs"]
mod tests;
