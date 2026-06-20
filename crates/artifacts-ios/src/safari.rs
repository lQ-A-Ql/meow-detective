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
mod tests {
    use super::*;
    use rusqlite::Connection;
    use std::io::Read;

    fn make_test_db() -> Vec<u8> {
        let mut tmp = tempfile::NamedTempFile::new().expect("tempfile");
        {
            let conn = Connection::open(tmp.path()).expect("open db");
            conn.execute_batch(
                "CREATE TABLE history_items (
                    id INTEGER PRIMARY KEY,
                    url TEXT,
                    title TEXT,
                    visit_count INTEGER DEFAULT 1
                );
                CREATE TABLE history_visits (
                    id INTEGER PRIMARY KEY,
                    history_item INTEGER,
                    visit_time REAL
                );

                INSERT INTO history_items VALUES (1, 'https://example.com', 'Example Domain', 12);
                INSERT INTO history_items VALUES (2, 'https://apple.com', 'Apple', 5);
                INSERT INTO history_items VALUES (3, 'https://github.com', '', 1);

                INSERT INTO history_visits VALUES (1, 1, 689500800.0);
                INSERT INTO history_visits VALUES (2, 2, 689860800.0);
                INSERT INTO history_visits VALUES (3, 1, 689508000.0);
                INSERT INTO history_visits VALUES (4, 3, 690000000.0);",
            )
            .expect("batch");
        }
        let mut buf = Vec::new();
        tmp.read_to_end(&mut buf).expect("read tmp");
        buf
    }

    fn make_empty_db() -> Vec<u8> {
        let mut tmp = tempfile::NamedTempFile::new().expect("tempfile");
        {
            let conn = Connection::open(tmp.path()).expect("open db");
            conn.execute_batch(
                "CREATE TABLE history_items (
                    id INTEGER PRIMARY KEY,
                    url TEXT,
                    title TEXT,
                    visit_count INTEGER DEFAULT 1
                );
                CREATE TABLE history_visits (
                    id INTEGER PRIMARY KEY,
                    history_item INTEGER,
                    visit_time REAL
                );",
            )
            .expect("batch");
        }
        let mut buf = Vec::new();
        tmp.read_to_end(&mut buf).expect("read tmp");
        buf
    }

    #[test]
    fn parse_safari_history_basic() {
        let db = make_test_db();
        let entries = parse_safari_history(&db).expect("parse safari history");
        assert_eq!(entries.len(), 4);

        // ORDER BY visit_time DESC:
        // 690000000.0 (github), 689860800.0 (apple), 689508000.0 (example), 689500800.0 (example)
        assert_eq!(entries[0].url, "https://github.com");
        assert_eq!(entries[0].visit_count, 1);
        assert!(entries[0].title.as_deref() == Some("") || entries[0].title.as_deref() == Some(""));

        assert_eq!(entries[1].url, "https://apple.com");
        assert_eq!(entries[1].title.as_deref(), Some("Apple"));
        assert_eq!(entries[1].visit_count, 5);

        assert_eq!(entries[2].url, "https://example.com");
        assert_eq!(entries[2].visit_count, 12);

        assert_eq!(entries[3].url, "https://example.com");
        assert_eq!(entries[3].visit_count, 12);

        // All should have timestamps
        for entry in &entries {
            assert!(entry.visit_time.is_some());
        }
    }

    #[test]
    fn parse_safari_history_empty_db() {
        let db = make_empty_db();
        let entries = parse_safari_history(&db).expect("parse");
        assert!(entries.is_empty());
    }

    #[test]
    fn parse_safari_history_not_a_db() {
        let result = parse_safari_history(b"garbage input");
        assert!(result.is_err());
    }

    #[test]
    fn parse_safari_history_no_visits() {
        let mut tmp = tempfile::NamedTempFile::new().expect("tempfile");
        {
            let conn = Connection::open(tmp.path()).expect("open db");
            conn.execute_batch(
                "CREATE TABLE history_items (
                    id INTEGER PRIMARY KEY,
                    url TEXT,
                    title TEXT,
                    visit_count INTEGER DEFAULT 1
                );
                CREATE TABLE history_visits (
                    id INTEGER PRIMARY KEY,
                    history_item INTEGER,
                    visit_time REAL
                );
                INSERT INTO history_items VALUES (1, 'https://orphan.com', 'Orphan', 3);",
            )
            .expect("batch");
        }
        let mut buf = Vec::new();
        tmp.read_to_end(&mut buf).expect("read tmp");
        let entries = parse_safari_history(&buf).expect("parse");
        // No visits → no joined rows
        assert!(entries.is_empty());
    }
}
