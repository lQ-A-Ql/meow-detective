//! Android SMS/MMS parser (mmssms.db).
//!
//! Parses the `mmssms.db` SQLite database found on Android devices:
//! `/data/data/com.android.providers.telephony/databases/mmssms.db`
//!
//! Key tables:
//! - `sms` — SMS messages (address, body, date, type, read, etc.)
//! - `mms` — MMS messages (subject, date, etc.)
//! - `addr` — MMS participant addresses
//! - `part` — MMS parts (attachments)
//!
//! SMS type constants:
//! - 1 = received
//! - 2 = sent
//! - 3 = draft
//! - 4 = outbox
//! - 5 = failed
//! - 6 = queued

use chrono::TimeZone;
use serde::{Deserialize, Serialize};
use std::io::Write;

/// A parsed Android SMS record.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AndroidSms {
    /// The phone number / contact address of the other party.
    pub address: Option<String>,
    /// The message body content.
    pub body: Option<String>,
    /// ISO 8601 timestamp of the message (converted from Unix millis).
    pub date: Option<String>,
    /// SMS type code: 1 = received, 2 = sent, 3 = draft, etc.
    pub sms_type: i32,
}

/// Parse an mmssms.db database from raw bytes.
pub fn parse_sms(data: &[u8]) -> Result<Vec<AndroidSms>, String> {
    if data.is_empty() {
        return Err("mmssms.db data is empty".to_string());
    }

    let mut tmp = tempfile::Builder::new()
        .suffix(".mmssms.db")
        .tempfile()
        .map_err(|e| format!("Failed to create temp file: {}", e))?;
    tmp.write_all(data)
        .map_err(|e| format!("Failed to write temp file: {}", e))?;
    tmp.flush()
        .map_err(|e| format!("Failed to flush temp file: {}", e))?;

    let conn = rusqlite::Connection::open(tmp.path())
        .map_err(|e| format!("Failed to open mmssms.db: {}", e))?;

    let msgs = query_sms(&conn)?;
    Ok(msgs)
}

fn query_sms(conn: &rusqlite::Connection) -> Result<Vec<AndroidSms>, String> {
    // Query the sms table. date is stored as Unix milliseconds since epoch.
    let mut stmt = conn
        .prepare("SELECT address, body, date, type FROM sms ORDER BY date DESC")
        .map_err(|e| format!("Failed to prepare sms query: {}", e))?;

    let msgs: Vec<AndroidSms> = stmt
        .query_map([], |row| {
            let address: Option<String> = row.get(0)?;
            let body: Option<String> = row.get(1)?;
            let date_millis: Option<i64> = row.get(2)?;
            let sms_type: i32 = row.get(3)?;

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

            Ok(AndroidSms {
                address,
                body,
                date,
                sms_type,
            })
        })
        .map_err(|e| format!("Failed to query sms: {}", e))?
        .filter_map(|r| r.ok())
        .collect();

    Ok(msgs)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_sms_test_db() -> Vec<u8> {
        let tmp = tempfile::Builder::new()
            .suffix(".mmssms.db")
            .tempfile()
            .expect("create temp file");
        let tmp_path = tmp.path().to_path_buf();
        drop(tmp);

        let conn = rusqlite::Connection::open(&tmp_path).expect("open temp db");

        conn.execute_batch(
            "CREATE TABLE sms (
                _id INTEGER PRIMARY KEY,
                address TEXT,
                body TEXT,
                date INTEGER,
                type INTEGER,
                read INTEGER DEFAULT 0
            );

            -- 2024-06-15 12:00:00 UTC = 1718452800000 ms
            INSERT INTO sms VALUES (1, '555-0100', 'Hey, how are you?', 1718452800000, 1, 1);
            -- 2024-06-15 12:05:00 UTC = 1718453100000 ms
            INSERT INTO sms VALUES (2, '555-0200', 'I''m fine, thanks!', 1718453100000, 2, 1);
            -- 2024-06-15 12:10:00 UTC = 1718453400000 ms
            INSERT INTO sms VALUES (3, '555-0300', 'Draft message', 1718453400000, 3, 0);
            ",
        )
        .expect("create test db");

        drop(conn);
        std::fs::read(&tmp_path).expect("read temp db")
    }

    #[test]
    fn parse_empty_data() {
        let result = parse_sms(&[]);
        assert!(result.is_err());
    }

    #[test]
    fn parse_sms_extracts_entries() {
        let data = build_sms_test_db();
        let msgs = parse_sms(&data).expect("should parse");
        assert_eq!(msgs.len(), 3, "Expected 3 SMS messages");

        // Received message
        let received = msgs
            .iter()
            .find(|m| m.sms_type == 1)
            .expect("received not found");
        assert_eq!(received.address.as_deref(), Some("555-0100"));
        assert_eq!(received.body.as_deref(), Some("Hey, how are you?"));
        assert_eq!(received.sms_type, 1);
        assert!(received.date.is_some());
        assert!(received.date.as_ref().unwrap().starts_with("2024-06-15"));

        // Sent message
        let sent = msgs
            .iter()
            .find(|m| m.sms_type == 2)
            .expect("sent not found");
        assert_eq!(sent.address.as_deref(), Some("555-0200"));
        assert_eq!(sent.body.as_deref(), Some("I'm fine, thanks!"));
        assert_eq!(sent.sms_type, 2);

        // Draft message
        let draft = msgs
            .iter()
            .find(|m| m.sms_type == 3)
            .expect("draft not found");
        assert_eq!(draft.sms_type, 3);
    }

    #[test]
    fn parse_invalid_sqlite_handles_gracefully() {
        let result = parse_sms(b"not a sqlite database");
        assert!(result.is_err());
    }

    #[test]
    fn parse_sms_with_null_fields() {
        let tmp = tempfile::Builder::new()
            .suffix(".nullsms.db")
            .tempfile()
            .expect("create temp file");
        let tmp_path = tmp.path().to_path_buf();
        drop(tmp);

        let conn = rusqlite::Connection::open(&tmp_path).expect("open temp db");
        conn.execute_batch(
            "CREATE TABLE sms (_id INTEGER PRIMARY KEY, address TEXT, body TEXT, date INTEGER, type INTEGER, read INTEGER DEFAULT 0);
             INSERT INTO sms VALUES (1, NULL, NULL, NULL, 1, 0);",
        )
        .expect("create test db");
        drop(conn);

        let data = std::fs::read(&tmp_path).expect("read temp db");
        let msgs = parse_sms(&data).expect("should parse");
        assert_eq!(msgs.len(), 1);
        assert!(msgs[0].address.is_none());
        assert!(msgs[0].body.is_none());
        assert!(msgs[0].date.is_none());
    }

    #[test]
    fn parse_sms_zero_date_returns_none() {
        let tmp = tempfile::Builder::new()
            .suffix(".zerodate.db")
            .tempfile()
            .expect("create temp file");
        let tmp_path = tmp.path().to_path_buf();
        drop(tmp);

        let conn = rusqlite::Connection::open(&tmp_path).expect("open temp db");
        conn.execute_batch(
            "CREATE TABLE sms (_id INTEGER PRIMARY KEY, address TEXT, body TEXT, date INTEGER, type INTEGER, read INTEGER DEFAULT 0);
             INSERT INTO sms VALUES (1, '555-0000', 'zero date', 0, 1, 0);",
        )
        .expect("create test db");
        drop(conn);

        let data = std::fs::read(&tmp_path).expect("read temp db");
        let msgs = parse_sms(&data).expect("should parse");
        assert_eq!(msgs.len(), 1);
        assert!(msgs[0].date.is_none());
    }
}
