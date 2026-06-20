//! Parse iOS Messages database (sms.db), extracting SMS and iMessage records
//! with sender, recipients, text payload, and timestamps.
//!
//! The `message` table holds message rows; the `handle` table maps `handle_id`
//! to recipient/sender identifiers (phone numbers or email addresses).  The
//! `chat` and `chat_handle_join` tables link conversations to recipients.

use crate::{open_sqlite_from_bytes, IosArtifactError};
use chrono::{DateTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};

/// A parsed iOS SMS or iMessage record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IosMessage {
    /// The sender identifier (phone number or email). `None` if unknown.
    pub sender: Option<String>,
    /// The recipient identifiers (phone numbers or emails).
    pub recipients: Vec<String>,
    /// The message text body. `None` if empty or attachment-only.
    pub text: Option<String>,
    /// The message timestamp.
    pub timestamp: Option<DateTime<Utc>>,
    /// Whether the message was sent by the device owner.
    pub is_from_me: bool,
}

/// Parse an iOS `sms.db` and return extracted messages.
///
/// Queries the `message` table for text, date, is_from_me, handle_id, and
/// joins against `handle` for sender identifier.  Additional recipients are
/// resolved via `chat_handle_join` and `chat_message_join` tables.
pub fn parse_messages(data: &[u8]) -> Result<Vec<IosMessage>, IosArtifactError> {
    let (conn, _tmp) = open_sqlite_from_bytes(data)?;

    // Handle lookup: ROWID → id (phone/email)
    let mut handles: std::collections::HashMap<i64, String> = std::collections::HashMap::new();
    if let Ok(mut stmt) = conn.prepare("SELECT ROWID, id FROM handle") {
        if let Ok(rows) = stmt.query_map([], |row| {
            let rowid: i64 = row.get(0)?;
            let id: String = row.get(1)?;
            Ok((rowid, id))
        }) {
            for row in rows.flatten() {
                handles.insert(row.0, row.1);
            }
        }
    }

    // chat_handle_join: chat_id → handle_id
    let mut chat_handles: std::collections::HashMap<i64, Vec<i64>> =
        std::collections::HashMap::new();
    if let Ok(mut stmt) = conn.prepare("SELECT chat_id, handle_id FROM chat_handle_join") {
        if let Ok(rows) = stmt.query_map([], |row| {
            let chat_id: i64 = row.get(0)?;
            let handle_id: i64 = row.get(1)?;
            Ok((chat_id, handle_id))
        }) {
            for row in rows.flatten() {
                chat_handles.entry(row.0).or_default().push(row.1);
            }
        }
    }

    // message → chat mapping via chat_message_join
    let mut msg_chats: std::collections::HashMap<i64, i64> = std::collections::HashMap::new();
    if let Ok(mut stmt) = conn.prepare("SELECT message_id, chat_id FROM chat_message_join") {
        if let Ok(rows) = stmt.query_map([], |row| {
            let msg_id: i64 = row.get(0)?;
            let chat_id: i64 = row.get(1)?;
            Ok((msg_id, chat_id))
        }) {
            for row in rows.flatten() {
                msg_chats.insert(row.0, row.1);
            }
        }
    }

    // Main message query
    let mut stmt = conn.prepare(
        "SELECT ROWID, text, date, is_from_me, handle_id FROM message ORDER BY date DESC",
    )?;

    let rows = stmt.query_map([], |row| {
        let rowid: i64 = row.get(0)?;
        let text: Option<String> = row.get(1).ok();
        let date_raw: i64 = row.get(2).unwrap_or(0);
        let is_from_me: bool = row.get(3).unwrap_or(false);
        let handle_id: Option<i64> = row.get(4).ok();
        Ok((rowid, text, date_raw, is_from_me, handle_id))
    })?;

    let mut results = Vec::new();
    for row in rows {
        let (rowid, text, date_raw, is_from_me, handle_id) = match row {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!("skipping message row: {}", e);
                continue;
            }
        };

        let sender = handle_id.and_then(|h| handles.get(&h).cloned());

        let mut recipients: Vec<String> = Vec::new();
        if let Some(chat_id) = msg_chats.get(&rowid) {
            if let Some(handle_ids) = chat_handles.get(chat_id) {
                for hid in handle_ids {
                    if let Some(id_str) = handles.get(hid) {
                        // Don't duplicate the sender in the recipient list.
                        if Some(id_str.as_str()) != sender.as_deref() {
                            recipients.push(id_str.clone());
                        }
                    }
                }
            }
        }

        let timestamp = if date_raw > 0 {
            // iOS message timestamps vary by OS version:
            //   - iOS < 11:  seconds since 2001-01-01 (CFAbsoluteTime)
            //   - iOS >= 11: nanoseconds since 2001-01-01 (CoreData)
            // Heuristic: values <  1e10 → CFAbsoluteTime seconds (~pre-2300).
            //            values >= 1e10 → CFAbsoluteTime nanoseconds.
            // Values in the Unix-seconds range (1e9..2e9) are ambiguous but
            // sms.db uses Apple epochs, not Unix epochs.
            if date_raw >= 10_000_000_000 {
                // Nanoseconds since 2001-01-01
                let unix_nanos = (date_raw as i128) + 978_307_200_000_000_000_i128;
                if unix_nanos > 0 {
                    let secs = (unix_nanos / 1_000_000_000) as i64;
                    let nsecs = (unix_nanos % 1_000_000_000) as u32;
                    chrono::Utc.timestamp_opt(secs, nsecs).single()
                } else {
                    None
                }
            } else {
                // Seconds since 2001-01-01
                crate::core_data_time_to_dt(date_raw as f64)
            }
        } else {
            None
        };

        results.push(IosMessage {
            sender,
            recipients,
            text,
            timestamp,
            is_from_me,
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
                "CREATE TABLE handle (
                    ROWID INTEGER PRIMARY KEY,
                    id TEXT
                );
                CREATE TABLE message (
                    ROWID INTEGER PRIMARY KEY,
                    text TEXT,
                    date INTEGER,
                    is_from_me INTEGER,
                    handle_id INTEGER
                );
                CREATE TABLE chat (
                    ROWID INTEGER PRIMARY KEY
                );
                CREATE TABLE chat_handle_join (
                    chat_id INTEGER,
                    handle_id INTEGER
                );
                CREATE TABLE chat_message_join (
                    chat_id INTEGER,
                    message_id INTEGER
                );

                -- Handles
                INSERT INTO handle VALUES (1, '+15551234567');
                INSERT INTO handle VALUES (2, '+15559876543');
                INSERT INTO handle VALUES (3, 'friend@example.com');

                -- Chat 100
                INSERT INTO chat VALUES (100);
                INSERT INTO chat_handle_join VALUES (100, 1);
                INSERT INTO chat_handle_join VALUES (100, 2);

                -- Chat 101
                INSERT INTO chat VALUES (101);
                INSERT INTO chat_handle_join VALUES (101, 1);
                INSERT INTO chat_handle_join VALUES (101, 3);

                -- Messages
                INSERT INTO message VALUES (1, 'Hey!', 689500800, 1, 2);      -- from me to handle 2
                INSERT INTO message VALUES (2, 'Hi there', 689504400, 0, 3);  -- from handle 3 to me
                INSERT INTO message VALUES (3, 'Meeting at 3?', 689508000, 1, 1); -- from me to handle 1
                INSERT INTO chat_message_join VALUES (100, 1);
                INSERT INTO chat_message_join VALUES (101, 2);
                INSERT INTO chat_message_join VALUES (100, 3);",
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
                "CREATE TABLE handle (ROWID INTEGER PRIMARY KEY, id TEXT);
                 CREATE TABLE message (ROWID INTEGER PRIMARY KEY, text TEXT, date INTEGER, is_from_me INTEGER, handle_id INTEGER);
                 CREATE TABLE chat (ROWID INTEGER PRIMARY KEY);
                 CREATE TABLE chat_handle_join (chat_id INTEGER, handle_id INTEGER);
                 CREATE TABLE chat_message_join (chat_id INTEGER, message_id INTEGER);",
            )
            .expect("batch");
        }
        let mut buf = Vec::new();
        tmp.read_to_end(&mut buf).expect("read tmp");
        buf
    }

    #[test]
    fn parse_messages_basic() {
        let db = make_test_db();
        let messages = parse_messages(&db).expect("parse messages");
        assert_eq!(messages.len(), 3);

        // ORDER BY date DESC → message 3 (latest), 2, 1
        assert_eq!(messages[0].text.as_deref(), Some("Meeting at 3?"));
        assert!(messages[0].is_from_me);
        assert_eq!(messages[0].sender.as_deref(), Some("+15551234567"));

        assert_eq!(messages[1].text.as_deref(), Some("Hi there"));
        assert!(!messages[1].is_from_me);
        assert_eq!(messages[1].sender.as_deref(), Some("friend@example.com"));

        assert_eq!(messages[2].text.as_deref(), Some("Hey!"));
        assert!(messages[2].is_from_me);
        assert_eq!(messages[2].sender.as_deref(), Some("+15559876543"));

        // Timestamps should be present
        assert!(messages[0].timestamp.is_some());
        assert!(messages[1].timestamp.is_some());
        assert!(messages[2].timestamp.is_some());
    }

    #[test]
    fn parse_messages_empty_db() {
        let db = make_empty_db();
        let messages = parse_messages(&db).expect("parse");
        assert!(messages.is_empty());
    }

    #[test]
    fn parse_messages_not_a_db() {
        let result = parse_messages(b"not a database");
        assert!(result.is_err());
    }

    #[test]
    fn parse_messages_no_handles() {
        let mut tmp = tempfile::NamedTempFile::new().expect("tempfile");
        {
            let conn = Connection::open(tmp.path()).expect("open db");
            conn.execute_batch(
                "CREATE TABLE handle (ROWID INTEGER PRIMARY KEY, id TEXT);
                 CREATE TABLE message (ROWID INTEGER PRIMARY KEY, text TEXT, date INTEGER, is_from_me INTEGER, handle_id INTEGER);
                 INSERT INTO message VALUES (1, 'orphan message', 689500800, 1, NULL);",
            )
            .expect("batch");
        }
        let mut buf = Vec::new();
        tmp.read_to_end(&mut buf).expect("read tmp");
        let messages = parse_messages(&buf).expect("parse");
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].text.as_deref(), Some("orphan message"));
        assert!(messages[0].sender.is_none());
        assert!(messages[0].recipients.is_empty());
    }

    #[test]
    fn parse_messages_coredata_timestamp() {
        let mut tmp = tempfile::NamedTempFile::new().expect("tempfile");
        {
            let conn = Connection::open(tmp.path()).expect("open db");
            conn.execute_batch(
                "CREATE TABLE handle (ROWID INTEGER PRIMARY KEY, id TEXT);
                 CREATE TABLE message (ROWID INTEGER PRIMARY KEY, text TEXT, date INTEGER, is_from_me INTEGER, handle_id INTEGER);
                 -- CoreData nanosecond timestamp (~2024-11-01 in ns since 2001)
                 INSERT INTO message VALUES (1, 'coredata msg', 753000000000000000, 1, NULL);",
            )
            .expect("batch");
        }
        let mut buf = Vec::new();
        tmp.read_to_end(&mut buf).expect("read tmp");
        let messages = parse_messages(&buf).expect("parse");
        assert_eq!(messages.len(), 1);
        // Large nanosecond value should produce a valid future timestamp
        assert!(messages[0].timestamp.is_some());
    }
}
