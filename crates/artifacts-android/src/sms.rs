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
#[path = "../tests/unit/sms.rs"]
mod tests;
