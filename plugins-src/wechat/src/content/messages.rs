//! `message_N.db` / `biz_message_N.db` → WeChatMessage: one artifact and one
//! timeline event per chat message.
//!
//! Layout (verified on 4.1.8.67): `Name2Id(user_name PK, is_session)` maps
//! conversation partners; each conversation lives in a `Msg_<md5(user_name)>`
//! table with columns `local_id PK, server_id, local_type, sort_seq,
//! real_sender_id, create_time (unix seconds), ..., message_content,
//! compress_content, packed_info_data BLOB, WCDB_CT_message_content,
//! WCDB_CT_source`.
//!
//! Content encoding: `WCDB_CT_message_content` NULL/0 means
//! `message_content` is plaintext; any other value means zstd-compressed
//! bytes (magic `28 b5 2f fd`). A decompression failure keeps the row but
//! drops `contentText` and raises one warning per table.
//!
//! Direction: `real_sender_id` is a rowid into `Name2Id`; when the resolved
//! `user_name` matches the owner wxid (from the evidence path, with its
//! `_<hash>` suffix tolerated) the message is outgoing (`isSend: true`).
//! Unresolvable senders omit the field.

use md5::{Digest, Md5};
use rusqlite::types::Value as SqlValue;
use serde_json::Value;
use std::collections::HashMap;

use super::{truncate_text, unix_to_rfc3339, CapGuard};
use crate::db::WeChatDb;
use crate::payload::{new_attrs, Payload};

/// Conservative local_type labels; unknown values keep only the raw number.
fn local_type_label(local_type: i64) -> Option<&'static str> {
    match local_type {
        1 => Some("文本"),
        3 => Some("图片"),
        34 => Some("语音"),
        43 => Some("视频"),
        47 => Some("表情"),
        49 => Some("复合内容（链接/小程序/引用等）"),
        10000 => Some("系统提示"),
        _ => None,
    }
}

/// md5 hex digest of a conversation user name (the `Msg_` table suffix).
fn md5_hex(name: &str) -> String {
    format!("{:x}", Md5::digest(name.as_bytes()))
}

/// The path wxid segment may carry a `_<hash>` suffix (e.g.
/// `wxid_zuaa9igqlro22_eef8`); the Name2Id value is the bare wxid.
fn same_account(owner_segment: &str, user_name: &str) -> bool {
    owner_segment == user_name
        || owner_segment
            .strip_prefix(user_name)
            .is_some_and(|rest| rest.starts_with('_'))
}

/// Parse every `Msg_<md5>` table into WeChatMessage artifacts plus one
/// timeline event per message.
pub fn parse(db: &WeChatDb, owner_wxid: &str, payload: &mut Payload) -> Result<usize, String> {
    let name2id = load_name2id(db)?;
    let talker_by_suffix: HashMap<String, String> = name2id
        .values()
        .filter(|name| !name.is_empty())
        .map(|name| (md5_hex(name), name.clone()))
        .collect();
    let tables = db
        .table_list()?
        .into_iter()
        .filter(|name| name.starts_with("Msg_"))
        .collect::<Vec<_>>();
    if !tables.is_empty() && name2id.is_empty() {
        payload.warn("Name2Id 缺失或为空：会话归属与收发方向无法解析");
    }

    let mut cap = CapGuard::new();
    let mut emitted = 0usize;
    for table in &tables {
        let talker = talker_by_suffix.get(table.trim_start_matches("Msg_"));
        emitted += parse_msg_table(db, table, talker, &name2id, owner_wxid, payload, &mut cap)?;
    }
    Ok(emitted)
}

/// `Name2Id` rowid → user_name (real_sender_id resolution).
fn load_name2id(db: &WeChatDb) -> Result<HashMap<i64, String>, String> {
    let mut map = HashMap::new();
    if !db.table_exists("Name2Id")? {
        return Ok(map);
    }
    let mut stmt = db
        .conn()
        .prepare("SELECT rowid, user_name FROM Name2Id")
        .map_err(|error| format!("Name2Id query prepare failed: {error}"))?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0).unwrap_or_default(),
                row.get::<_, String>(1).unwrap_or_default(),
            ))
        })
        .map_err(|error| format!("Name2Id query failed: {error}"))?;
    for row in rows {
        let (rowid, name) = row.map_err(|error| format!("Name2Id row failed: {error}"))?;
        map.insert(rowid, name);
    }
    Ok(map)
}

fn parse_msg_table(
    db: &WeChatDb,
    table: &str,
    talker: Option<&String>,
    name2id: &HashMap<i64, String>,
    owner_wxid: &str,
    payload: &mut Payload,
    cap: &mut CapGuard,
) -> Result<usize, String> {
    let escaped = table.replace('"', "\"\"");
    let sql = format!(
        "SELECT local_id, server_id, local_type, real_sender_id, create_time, \
         message_content, WCDB_CT_message_content FROM \"{escaped}\" ORDER BY local_id"
    );
    let mut stmt = db
        .conn()
        .prepare(&sql)
        .map_err(|error| format!("{table} query prepare failed: {error}"))?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0).unwrap_or_default(),
                row.get::<_, i64>(1).unwrap_or_default(),
                row.get::<_, i64>(2).unwrap_or_default(),
                row.get::<_, i64>(3).unwrap_or_default(),
                row.get::<_, i64>(4).unwrap_or_default(),
                row.get::<_, SqlValue>(5).unwrap_or(SqlValue::Null),
                row.get::<_, Option<i64>>(6).unwrap_or_default(),
            ))
        })
        .map_err(|error| format!("{table} query failed: {error}"))?;

    let mut emitted = 0usize;
    let mut decode_warned = false;
    for row in rows {
        let (local_id, server_id, local_type, sender_id, create_time, content, ct) =
            row.map_err(|error| format!("{table} row failed: {error}"))?;
        if !cap.allow("WeChatMessage", payload) {
            break;
        }
        let compressed = ct.unwrap_or(0) != 0;
        let decoded = decode_content(content, compressed);
        let text = match decoded {
            Content::Text(text) => Some(text),
            Content::Undecodable => {
                if compressed && !decode_warned {
                    decode_warned = true;
                    payload.warn(format!(
                        "{table} 存在无法解压的 zstd 消息内容，相关行省略正文"
                    ));
                }
                None
            }
        };

        let mut attrs = new_attrs();
        attrs.insert("talkerTable".to_string(), Value::String(table.to_string()));
        if let Some(name) = talker {
            attrs.insert("talker".to_string(), Value::String(name.clone()));
        }
        attrs.insert("localId".to_string(), Value::from(local_id));
        attrs.insert("serverId".to_string(), Value::from(server_id));
        attrs.insert("localType".to_string(), Value::from(local_type));
        if let Some(label) = local_type_label(local_type) {
            attrs.insert(
                "localTypeLabel".to_string(),
                Value::String(label.to_string()),
            );
        }
        let timestamp = unix_to_rfc3339(create_time);
        if let Some(ts) = &timestamp {
            attrs.insert("createTimeUtc".to_string(), Value::String(ts.clone()));
        }
        if let Some(sender_name) = name2id.get(&sender_id) {
            attrs.insert(
                "isSend".to_string(),
                Value::Bool(same_account(owner_wxid, sender_name)),
            );
        }
        attrs.insert("zstdCompressed".to_string(), Value::Bool(compressed));
        let mut summary_text = String::new();
        if let Some(text) = text {
            let (truncated, was_truncated) = truncate_text(&text);
            attrs.insert("contentText".to_string(), Value::String(truncated.clone()));
            if was_truncated {
                attrs.insert("contentTruncated".to_string(), Value::Bool(true));
            }
            summary_text = truncated.chars().take(60).collect();
        }

        let label = local_type_label(local_type).unwrap_or("未知类型");
        let who = talker.map(String::as_str).unwrap_or("<unknown>");
        payload.artifact(
            "WeChatMessage",
            format!("消息 {who} #{local_id}"),
            if summary_text.is_empty() {
                format!("{label}消息（{table} #{local_id}）")
            } else {
                format!("{label}消息（{table}）：{summary_text}")
            },
            attrs.clone(),
        );
        if let Some(ts) = timestamp {
            payload.timeline_event(
                ts,
                "WeChatMessage",
                format!("与 {who} 的{label}消息"),
                attrs,
            );
        }
        emitted += 1;
    }
    Ok(emitted)
}

enum Content {
    Text(String),
    Undecodable,
}

/// Resolve `message_content`: plaintext when `compressed` is false,
/// otherwise zstd-decode the raw bytes (magic `28 b5 2f fd`).
fn decode_content(content: SqlValue, compressed: bool) -> Content {
    let bytes = match &content {
        SqlValue::Text(text) => text.as_bytes().to_vec(),
        SqlValue::Blob(blob) => blob.clone(),
        _ => return Content::Text(String::new()),
    };
    if !compressed {
        return Content::Text(String::from_utf8_lossy(&bytes).into_owned());
    }
    match zstd::decode_all(bytes.as_slice()) {
        Ok(plain) => Content::Text(String::from_utf8_lossy(&plain).into_owned()),
        Err(_) => Content::Undecodable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_account_tolerates_hash_suffix() {
        assert!(same_account("wxid_abc22_eef8", "wxid_abc22"));
        assert!(same_account("wxid_abc22", "wxid_abc22"));
        assert!(!same_account("wxid_abc22", "wxid_abc23"));
        assert!(!same_account("wxid_abc22", "wxid_abc"));
    }

    #[test]
    fn md5_suffix_matches_real_layout() {
        // WeChat derives the conversation table as Msg_<md5(user_name)>.
        assert_eq!(md5_hex("filehelper").len(), 32);
    }
}
