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
#[path = "../tests/unit/messages.rs"]
mod tests;
