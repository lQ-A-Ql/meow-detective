//! macOS Quarantine Events (QuarantineEventsV2) parser.
//!
//! Parses the `com.apple.quarantineevents` SQLite database found at:
//! `~/Library/Preferences/com.apple.LaunchServices.QuarantineEventsV2`
//!
//! This database records files downloaded from the internet along with their
//! origin URL, originating bundle (browser/application), and quarantine agent.
//!
//! Table: `LSQuarantineEvent`
//! Columns:
//! - `LSQuarantineEventIdentifier` (TEXT) — UUID
//! - `LSQuarantineTimeStamp` (REAL) — seconds since 2001-01-01 (Apple epoch)
//! - `LSQuarantineAgentBundleIdentifier` (TEXT) — agent bundle ID
//! - `LSQuarantineAgentName` (TEXT) — agent name
//! - `LSQuarantineDataURLString` (TEXT) — origin URL
//! - `LSQuarantineOriginURLString` (TEXT) — referrer URL
//! - `LSQuarantineSenderName` (TEXT) — sender name
//! - `LSQuarantineOriginSenderName` (TEXT) — origin sender
//! - `LSQuarantineTypeNumber` (INTEGER) — quarantine type

use crate::error::{MacArtifactError, Result};
use chrono::{TimeZone, Utc};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

/// A single quarantine event entry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct QuarantineEntry {
    /// The download/origin URL that triggered quarantine
    pub url: String,
    /// The originating application bundle ID (e.g., "com.google.Chrome")
    pub origin_bundle: String,
    /// The quarantine agent (typically the same as origin_bundle)
    pub agent: String,
    /// ISO 8601 timestamp of the event
    pub timestamp: String,
}

/// Apple epoch: 2001-01-01T00:00:00Z in seconds since Unix epoch.
const APPLE_EPOCH_OFFSET: f64 = 978_307_200.0;

/// Parse a QuarantineEventsV2 database from raw bytes.
///
/// Opens the SQLite database in memory and queries the `LSQuarantineEvent` table
/// to extract quarantine event records.
pub fn parse_quarantine_events(data: &[u8]) -> Result<Vec<QuarantineEntry>> {
    if data.is_empty() {
        return Err(MacArtifactError::InvalidInput(
            "QuarantineEvents database is empty".to_string(),
        ));
    }

    // Write the data to a temp file and open it as SQLite
    use std::io::Write;
    let mut tmp = tempfile::Builder::new()
        .suffix(".quarantine.db")
        .tempfile()?;
    tmp.write_all(data)?;
    tmp.flush()?;

    let conn = Connection::open(tmp.path()).map_err(MacArtifactError::Database)?;

    // Check if the expected table exists
    let tables = get_quarantine_tables(&conn)?;

    let has_quarantine_table = tables.iter().any(|t| t.contains("Quarantine"));

    if !has_quarantine_table {
        return Err(MacArtifactError::InvalidInput(
            "No LSQuarantineEvent table found in database".to_string(),
        ));
    }

    let entries = query_quarantine_events(&conn)?;
    Ok(entries)
}

/// Get table names from the database.
fn get_quarantine_tables(conn: &Connection) -> Result<Vec<String>> {
    let mut stmt = conn.prepare("SELECT name FROM sqlite_master WHERE type='table'")?;

    let names: Vec<String> = stmt
        .query_map([], |row| row.get(0))?
        .filter_map(|r| r.ok())
        .collect();

    Ok(names)
}

/// Query the LSQuarantineEvent table for entries.
fn query_quarantine_events(conn: &Connection) -> Result<Vec<QuarantineEntry>> {
    // First try the standard schema
    let query = "SELECT LSQuarantineDataURLString, LSQuarantineAgentBundleIdentifier, LSQuarantineAgentName, LSQuarantineTimeStamp FROM LSQuarantineEvent LIMIT 1000";

    let mut stmt = match conn.prepare(query) {
        Ok(s) => s,
        Err(_) => {
            // Try alternative schema with different column names
            let alt_query = "SELECT LSQuarantineOriginURLString, LSQuarantineAgentBundleIdentifier, LSQuarantineAgentName, LSQuarantineTimeStamp FROM LSQuarantineEvent LIMIT 1000";
            conn.prepare(alt_query)
                .map_err(MacArtifactError::Database)?
        }
    };

    let entries: Vec<QuarantineEntry> = stmt
        .query_map([], |row| {
            let url: String = row.get(0).unwrap_or_default();
            let origin_bundle: String = row.get(1).unwrap_or_default();
            let agent: String = row.get(2).unwrap_or_else(|_| origin_bundle.clone());
            let timestamp_raw: Option<f64> = row.get(3).ok();

            let timestamp = timestamp_raw
                .and_then(convert_apple_timestamp)
                .unwrap_or_else(|| "unknown".to_string());

            Ok(QuarantineEntry {
                url,
                origin_bundle,
                agent,
                timestamp,
            })
        })
        .map_err(MacArtifactError::Database)?
        .filter_map(|r| r.ok())
        .collect();

    Ok(entries)
}

/// Convert Apple epoch timestamp (seconds since 2001-01-01) to ISO 8601.
fn convert_apple_timestamp(apple_ts: f64) -> Option<String> {
    let unix_ts = apple_ts + APPLE_EPOCH_OFFSET;
    if unix_ts < 0.0 {
        return None;
    }
    if unix_ts > (i64::MAX as f64) {
        return None;
    }
    let secs = unix_ts as i64;
    let nanos = ((unix_ts - unix_ts.floor()) * 1_000_000_000.0) as u32;
    Utc.timestamp_opt(secs, nanos)
        .single()
        .map(|dt| dt.to_rfc3339())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal QuarantineEventsV2 test database.
    /// Writes to a temp file and reads back the bytes.
    fn build_quarantine_test_db() -> Vec<u8> {
        let tmp = tempfile::Builder::new()
            .suffix(".quarantine.db")
            .tempfile()
            .expect("create temp file");
        let tmp_path = tmp.path().to_path_buf();
        drop(tmp);

        let conn = Connection::open(&tmp_path).expect("open temp db");

        conn.execute_batch(
            "CREATE TABLE LSQuarantineEvent (
                LSQuarantineEventIdentifier TEXT PRIMARY KEY,
                LSQuarantineTimeStamp REAL,
                LSQuarantineAgentBundleIdentifier TEXT,
                LSQuarantineAgentName TEXT,
                LSQuarantineDataURLString TEXT,
                LSQuarantineOriginURLString TEXT,
                LSQuarantineSenderName TEXT,
                LSQuarantineOriginSenderName TEXT,
                LSQuarantineTypeNumber INTEGER
            );

            INSERT INTO LSQuarantineEvent VALUES
                ('uuid-1', 696902400.0, 'com.google.Chrome', 'Google Chrome',
                 'https://example.com/file.dmg', '', 'Example Site', '', 0),
                ('uuid-2', 697000000.0, 'com.apple.Safari', 'Safari',
                 'https://download.example.org/app.pkg', 'https://referrer.example.com',
                 'Download Site', '', 0),
                ('uuid-3', 697100000.0, 'com.apple.mail', 'Mail',
                 'https://cdn.example.net/doc.zip', '', '', '', 0);
            ",
        )
        .expect("create test db");

        drop(conn);

        std::fs::read(&tmp_path).expect("read temp db")
    }

    #[test]
    fn parse_empty_data() {
        let result = parse_quarantine_events(&[]);
        assert!(result.is_err());
    }

    #[test]
    fn parse_quarantine_events_extracts_entries() {
        let data = build_quarantine_test_db();
        let entries = parse_quarantine_events(&data).expect("should parse");

        assert!(
            !entries.is_empty(),
            "Expected at least one quarantine event"
        );
        assert_eq!(entries.len(), 3);

        let first = &entries[0];
        assert_eq!(first.url, "https://example.com/file.dmg");
        assert_eq!(first.origin_bundle, "com.google.Chrome");
        assert_eq!(first.agent, "Google Chrome");
        assert!(!first.timestamp.is_empty());
        assert_ne!(first.timestamp, "unknown");
    }

    #[test]
    fn parse_invalid_sqlite_handles_gracefully() {
        let result = parse_quarantine_events(b"not a sqlite database");
        assert!(result.is_err());
    }

    #[test]
    fn convert_apple_timestamp_valid() {
        // 2024-01-15 00:00:00 UTC
        // Apple epoch to Unix: 2024-01-15 00:00:00 UTC - 2001-01-01 00:00:00 UTC
        // = 23 years + 14 days = 23*365 + 5 leap days (2004,2008,2012,2016,2020) + 14 days
        // = 8395 + 5 + 14 = 8414 days = 8414 * 86400 = 726,969,600
        let apple_ts = 726_969_600.0;
        let result = convert_apple_timestamp(apple_ts);
        assert!(result.is_some());
        let iso = result.unwrap();
        assert!(
            iso.starts_with("2024-01-15"),
            "Expected 2024-01-15 timestamp, got: {}",
            iso
        );
    }

    #[test]
    fn convert_apple_timestamp_large_negative_returns_none() {
        // A timestamp before the Apple epoch by more than the offset
        // would produce a negative Unix timestamp, which is rejected
        let result = convert_apple_timestamp(-1_000_000_000.0);
        assert!(result.is_none());
    }
}
