//! Content parsers for decrypted (or natively plaintext) WeChat 4.x
//! databases — one file per artifact family.
//!
//! All parsers run against the in-memory deserialized copy (`db.rs`) and
//! never touch the host disk. Output is capped at `MAX_ARTIFACTS_PER_DB`
//! artifacts per database; overflow is truncated with one warning. Schema
//! drift (a missing table or column) degrades to a warning plus whatever
//! rows could be read, never a panic.

pub mod contacts;
pub mod favorites;
pub mod messages;
pub mod sessions;
pub mod sns;

use serde_json::{Map, Value};

use crate::db::WeChatDb;
use crate::payload::Payload;

/// Hard cap on content artifacts emitted for a single database; the host
/// reads the whole payload into memory, so a pathological database must not
/// blow it up.
pub const MAX_ARTIFACTS_PER_DB: usize = 20_000;

/// Long text fields (message/favorite bodies) are truncated to this many
/// Unicode scalar values.
pub const MAX_TEXT_CHARS: usize = 500;

/// Dispatch a plaintext database to its content parser by file name.
/// Unknown databases (fts/resource indexes, future splits) are left to the
/// generic schema inventory in `parse.rs`.
pub fn parse_content(
    db_name: &str,
    owner_wxid: &str,
    db: &WeChatDb,
    payload: &mut Payload,
) -> Result<(), String> {
    let lower = db_name.to_ascii_lowercase();
    if lower == "contact.db" {
        contacts::parse(db, payload)?;
    } else if lower == "session.db" {
        sessions::parse(db, payload)?;
    } else if lower == "sns.db" {
        sns::parse(db, payload)?;
    } else if lower == "favorite.db" {
        favorites::parse(db, payload)?;
    } else if is_message_db(&lower) {
        messages::parse(db, owner_wxid, payload)?;
    }
    Ok(())
}

/// `message_N.db` / `biz_message_N.db` hold per-conversation `Msg_` tables;
/// the FTS/resource companions stay inventory-only.
fn is_message_db(lower_name: &str) -> bool {
    (lower_name.starts_with("message_") || lower_name.starts_with("biz_message_"))
        && lower_name.ends_with(".db")
        && !lower_name.contains("fts")
        && !lower_name.contains("resource")
}

/// unix seconds → UTC RFC3339 (`2026-03-30T05:47:22Z`). Out-of-range or
/// non-positive values yield `None` and the field/event is omitted.
pub fn unix_to_rfc3339(secs: i64) -> Option<String> {
    if secs <= 0 {
        return None;
    }
    chrono::DateTime::from_timestamp(secs, 0).map(|ts| ts.format("%Y-%m-%dT%H:%M:%SZ").to_string())
}

/// Char-boundary-safe truncation to `MAX_TEXT_CHARS`; the bool reports
/// whether truncation happened.
pub fn truncate_text(text: &str) -> (String, bool) {
    if text.chars().count() <= MAX_TEXT_CHARS {
        return (text.to_string(), false);
    }
    (text.chars().take(MAX_TEXT_CHARS).collect(), true)
}

/// Insert a trimmed string attr only when non-empty.
pub fn insert_text(attrs: &mut Map<String, Value>, key: &str, value: &str) {
    let trimmed = value.trim();
    if !trimmed.is_empty() {
        attrs.insert(key.to_string(), Value::String(trimmed.to_string()));
    }
}

/// Per-database artifact cap guard shared by all content parsers.
pub struct CapGuard {
    emitted: usize,
    warned: bool,
}

impl Default for CapGuard {
    fn default() -> Self {
        Self::new()
    }
}

impl CapGuard {
    pub fn new() -> Self {
        Self {
            emitted: 0,
            warned: false,
        }
    }

    /// Whether another artifact may be emitted; on the first overflow a
    /// truncation warning is pushed and further emission stops.
    pub fn allow(&mut self, family: &str, payload: &mut Payload) -> bool {
        if self.emitted < MAX_ARTIFACTS_PER_DB {
            self.emitted += 1;
            return true;
        }
        if !self.warned {
            self.warned = true;
            payload.warn(format!(
                "{family} 产物超过单库上限 {MAX_ARTIFACTS_PER_DB} 条，超出部分已截断"
            ));
        }
        false
    }
}
