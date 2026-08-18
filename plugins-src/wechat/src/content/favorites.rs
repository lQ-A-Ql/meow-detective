//! `favorite.db` → WeChatFavorite: one artifact per row of `fav_db_item`.
//!
//! Schema: `fav_db_item(local_id PK, server_id, type, update_seq, flag,
//! update_time (unix seconds), version, content TEXT, source_id,
//! sync_status, ..., fromusr, realchatname, ext_buf)`. Empty favorites
//! databases are common (the table may have zero rows) and simply emit
//! nothing.

use serde_json::Value;

use super::{insert_text, unix_to_rfc3339, CapGuard};
use crate::db::WeChatDb;
use crate::payload::{new_attrs, Payload};

/// Parse `fav_db_item` into WeChatFavorite artifacts.
pub fn parse(db: &WeChatDb, payload: &mut Payload) -> Result<usize, String> {
    if !db.table_exists("fav_db_item")? {
        return Ok(0);
    }
    let mut stmt = db
        .conn()
        .prepare(
            "SELECT local_id, server_id, type, update_time, fromusr, \
             realchatname, content FROM fav_db_item ORDER BY local_id",
        )
        .map_err(|error| format!("fav_db_item query prepare failed: {error}"))?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0).unwrap_or_default(),
                row.get::<_, i64>(1).unwrap_or_default(),
                row.get::<_, i64>(2).unwrap_or_default(),
                row.get::<_, i64>(3).unwrap_or_default(),
                row.get::<_, String>(4).unwrap_or_default(),
                row.get::<_, String>(5).unwrap_or_default(),
                row.get::<_, String>(6).unwrap_or_default(),
            ))
        })
        .map_err(|error| format!("fav_db_item query failed: {error}"))?;

    let mut cap = CapGuard::new();
    let mut emitted = 0usize;
    for row in rows {
        let (local_id, server_id, fav_type, update_time, fromusr, realchatname, content) =
            row.map_err(|error| format!("fav_db_item row failed: {error}"))?;
        if !cap.allow("WeChatFavorite", payload) {
            break;
        }
        let mut attrs = new_attrs();
        attrs.insert("localId".to_string(), Value::from(local_id));
        attrs.insert("serverId".to_string(), Value::from(server_id));
        attrs.insert("type".to_string(), Value::from(fav_type));
        insert_text(&mut attrs, "fromUsr", &fromusr);
        insert_text(&mut attrs, "realChatName", &realchatname);
        if let Some(ts) = unix_to_rfc3339(update_time) {
            attrs.insert("updateTimeUtc".to_string(), Value::String(ts));
        }
        insert_text(&mut attrs, "contentText", &content);
        let who = if fromusr.trim().is_empty() {
            "<unknown>"
        } else {
            fromusr.trim()
        };
        payload.artifact(
            "WeChatFavorite",
            format!("收藏 #{local_id}"),
            format!("微信收藏（来自 {who}，类型 {fav_type}）"),
            attrs,
        );
        emitted += 1;
    }
    Ok(emitted)
}
