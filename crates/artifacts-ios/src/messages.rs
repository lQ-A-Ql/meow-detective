//! Parse iOS Messages database (sms.db), extracting SMS and iMessage records
//! with sender, recipients, text payload, and timestamps.
//!
//! The `message` table holds message rows; the `handle` table maps `handle_id`
//! to recipient/sender identifiers (phone numbers or email addresses).  The
//! `chat` and `chat_handle_join` tables link conversations to recipients.

use crate::{open_sqlite_from_bytes, IosArtifactError};
use chrono::{DateTime, TimeZone, Utc};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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
    let handles = load_pairs(&conn, "SELECT ROWID, id FROM handle");
    let chat_handles = load_multi_pairs(&conn, "SELECT chat_id, handle_id FROM chat_handle_join");
    let message_chats = load_pairs(&conn, "SELECT message_id, chat_id FROM chat_message_join");
    let mut stmt = conn.prepare(
        "SELECT ROWID, text, date, is_from_me, handle_id FROM message ORDER BY date DESC",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(MessageRow {
            row_id: row.get(0)?,
            text: row.get(1).ok(),
            date_raw: row.get(2).unwrap_or(0),
            is_from_me: row.get(3).unwrap_or(false),
            handle_id: row.get(4).ok(),
        })
    })?;
    let mut results = Vec::new();
    for row in rows {
        match row {
            Ok(row) => results.push(build_message(row, &handles, &chat_handles, &message_chats)),
            Err(error) => {
                tracing::warn!("skipping message row: {}", error);
                continue;
            }
        }
    }
    Ok(results)
}

struct MessageRow {
    row_id: i64,
    text: Option<String>,
    date_raw: i64,
    is_from_me: bool,
    handle_id: Option<i64>,
}

fn load_pairs<V>(conn: &Connection, query: &str) -> HashMap<i64, V>
where
    V: rusqlite::types::FromSql,
{
    let Ok(mut stmt) = conn.prepare(query) else {
        return HashMap::new();
    };
    let Ok(rows) = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?))) else {
        return HashMap::new();
    };
    rows.flatten().collect()
}

fn load_multi_pairs(conn: &Connection, query: &str) -> HashMap<i64, Vec<i64>> {
    let mut result = HashMap::new();
    for (key, value) in load_pair_rows(conn, query) {
        result.entry(key).or_insert_with(Vec::new).push(value);
    }
    result
}

fn load_pair_rows(conn: &Connection, query: &str) -> Vec<(i64, i64)> {
    let Ok(mut stmt) = conn.prepare(query) else {
        return Vec::new();
    };
    let Ok(rows) = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?))) else {
        return Vec::new();
    };
    rows.flatten().collect()
}

fn build_message(
    row: MessageRow,
    handles: &HashMap<i64, String>,
    chat_handles: &HashMap<i64, Vec<i64>>,
    message_chats: &HashMap<i64, i64>,
) -> IosMessage {
    let sender = row.handle_id.and_then(|id| handles.get(&id).cloned());
    let recipients = message_chats
        .get(&row.row_id)
        .and_then(|chat_id| chat_handles.get(chat_id))
        .into_iter()
        .flatten()
        .filter_map(|handle_id| handles.get(handle_id))
        .filter(|identifier| Some(identifier.as_str()) != sender.as_deref())
        .cloned()
        .collect();
    IosMessage {
        sender,
        recipients,
        text: row.text,
        timestamp: parse_message_timestamp(row.date_raw),
        is_from_me: row.is_from_me,
    }
}

fn parse_message_timestamp(raw: i64) -> Option<DateTime<Utc>> {
    if raw <= 0 {
        return None;
    }
    if raw < 10_000_000_000 {
        return crate::core_data_time_to_dt(raw as f64);
    }
    let unix_nanos = (raw as i128) + 978_307_200_000_000_000_i128;
    (unix_nanos > 0).then(|| {
        let secs = (unix_nanos / 1_000_000_000) as i64;
        let nanos = (unix_nanos % 1_000_000_000) as u32;
        Utc.timestamp_opt(secs, nanos).single()
    })?
}

#[cfg(test)]
#[path = "../tests/unit/messages.rs"]
mod tests;
