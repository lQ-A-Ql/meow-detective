//! `session.db` → WeChatSession: one artifact per row of `SessionTable`.
//!
//! Schema (verified on 4.1.8.67): `SessionTable(username PK, type,
//! unread_count, ..., is_hidden, summary, draft, status, last_timestamp,
//! sort_timestamp, ..., last_msg_sender, last_sender_display_name,
//! last_msg_type, last_msg_sub_type)`. `last_timestamp` is unix seconds.

use serde_json::Value;

use super::{insert_text, unix_to_rfc3339, CapGuard};
use crate::db::WeChatDb;
use crate::payload::{new_attrs, Payload};

/// Parse `SessionTable` into WeChatSession artifacts.
pub fn parse(db: &WeChatDb, payload: &mut Payload) -> Result<usize, String> {
    if !db.table_exists("SessionTable")? {
        return Ok(0);
    }
    let mut stmt = db
        .conn()
        .prepare(
            "SELECT username, type, unread_count, is_hidden, summary, \
             last_timestamp, last_msg_sender, last_sender_display_name, \
             last_msg_type FROM SessionTable",
        )
        .map_err(|error| format!("SessionTable query prepare failed: {error}"))?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0).unwrap_or_default(),
                row.get::<_, i64>(1).unwrap_or_default(),
                row.get::<_, i64>(2).unwrap_or_default(),
                row.get::<_, i64>(3).unwrap_or_default(),
                row.get::<_, String>(4).unwrap_or_default(),
                row.get::<_, i64>(5).unwrap_or_default(),
                row.get::<_, String>(6).unwrap_or_default(),
                row.get::<_, String>(7).unwrap_or_default(),
                row.get::<_, i64>(8).unwrap_or_default(),
            ))
        })
        .map_err(|error| format!("SessionTable query failed: {error}"))?;

    let mut cap = CapGuard::new();
    let mut emitted = 0usize;
    for row in rows {
        let (username, stype, unread, hidden, summary, last_ts, _last_sender, display, msg_type) =
            row.map_err(|error| format!("SessionTable row failed: {error}"))?;
        if !cap.allow("WeChatSession", payload) {
            break;
        }
        let mut attrs = new_attrs();
        attrs.insert("username".to_string(), Value::String(username.clone()));
        attrs.insert("type".to_string(), Value::from(stype));
        attrs.insert("unreadCount".to_string(), Value::from(unread));
        attrs.insert("isHidden".to_string(), Value::Bool(hidden != 0));
        attrs.insert("lastMsgType".to_string(), Value::from(msg_type));
        insert_text(&mut attrs, "summary", &summary);
        insert_text(&mut attrs, "lastSenderDisplayName", &display);
        if let Some(ts) = unix_to_rfc3339(last_ts) {
            attrs.insert("lastTimestampUtc".to_string(), Value::String(ts));
        }
        let shown = if display.trim().is_empty() {
            username.clone()
        } else {
            display.trim().to_string()
        };
        payload.artifact(
            "WeChatSession",
            format!("会话 {shown}"),
            format!("微信会话 {shown}（未读 {unread}）"),
            attrs,
        );
        emitted += 1;
    }
    Ok(emitted)
}
