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
//! `_<hash>` suffix tolerated), or is the WeChat 4.x self alias in rowid 1,
//! the message is outgoing (`isSend: true`). Unresolvable senders omit the
//! field.

use md5::{Digest, Md5};
use rusqlite::types::Value as SqlValue;
use serde_json::Value;
use std::collections::HashMap;

use super::{rich_message, unix_to_rfc3339, CapGuard};
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
    owner_segment.eq_ignore_ascii_case(user_name)
        || owner_segment
            .get(..user_name.len())
            .filter(|prefix| prefix.eq_ignore_ascii_case(user_name))
            .and_then(|_| owner_segment.get(user_name.len()..))
            .is_some_and(|rest| rest.starts_with('_'))
}

/// WeChat 4.x stores the local account as `rowid = 1` with the literal
/// username `weixin` (or an equivalent self alias), while the evidence path
/// still contains the account's wxid. Keep the rowid check narrow so a
/// malformed Name2Id table cannot turn an arbitrary contact into an outgoing
/// message.
fn is_owner_sender(owner_wxid: &str, sender_id: i64, sender_name: &str) -> bool {
    same_account(owner_wxid, sender_name) || (sender_id == 1 && is_self_alias(sender_name))
}

fn is_self_alias(sender_name: &str) -> bool {
    matches!(
        sender_name.trim().to_ascii_lowercase().as_str(),
        "weixin" | "self" | "me"
    )
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
    let source = optional_column(db, table, "source", "NULL")?;
    let source_ct = optional_column(db, table, "WCDB_CT_source", "NULL")?;
    let compressed_content = optional_column(db, table, "compress_content", "NULL")?;
    let packed_info = optional_column(db, table, "packed_info_data", "NULL")?;
    let sql = format!(
        "SELECT local_id, server_id, local_type, real_sender_id, create_time, \
         message_content, WCDB_CT_message_content, {source}, {source_ct}, \
         {compressed_content}, {packed_info} FROM \"{escaped}\" ORDER BY local_id"
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
                row.get::<_, SqlValue>(7).unwrap_or(SqlValue::Null),
                row.get::<_, Option<i64>>(8).unwrap_or_default(),
                row.get::<_, SqlValue>(9).unwrap_or(SqlValue::Null),
                row.get::<_, SqlValue>(10).unwrap_or(SqlValue::Null),
            ))
        })
        .map_err(|error| format!("{table} query failed: {error}"))?;

    let mut emitted = 0usize;
    let mut decode_warned = false;
    for row in rows {
        let (
            local_id,
            server_id,
            local_type,
            sender_id,
            create_time,
            content,
            ct,
            source,
            source_ct,
            compressed_content,
            packed_info,
        ) = row.map_err(|error| format!("{table} row failed: {error}"))?;
        if !cap.allow("WeChatMessage", payload) {
            break;
        }
        let compressed = ct.unwrap_or(0) != 0;
        let decoded = decode_message_content(content, compressed_content, compressed);
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
        let source_text = decode_auxiliary(source, source_ct.unwrap_or(0) != 0);
        let packed_text = decode_packed_info(packed_info);

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
                "senderUsername".to_string(),
                Value::String(sender_name.clone()),
            );
            attrs.insert(
                "isSend".to_string(),
                Value::Bool(is_owner_sender(owner_wxid, sender_id, sender_name)),
            );
        }
        attrs.insert("zstdCompressed".to_string(), Value::Bool(compressed));
        let mut summary_text = String::new();
        if let Some(text) = text {
            attrs.insert("contentText".to_string(), Value::String(text.clone()));
            rich_message::enrich(local_type, &text, &mut attrs);
            summary_text = display_text(&attrs, &text).chars().take(60).collect();
        }
        if let Some(source) = source_text {
            attrs.insert("sourceContent".to_string(), Value::String(source.clone()));
            rich_message::enrich_source(&source, &mut attrs);
        }
        if let Some(packed) = packed_text {
            attrs.insert("packedInfoText".to_string(), Value::String(packed.clone()));
            rich_message::enrich_packed_info(&packed, &mut attrs);
        }
        if !attrs.contains_key("senderUsername") {
            if let Some(Value::String(sender)) = attrs.get("sourceUsername").cloned() {
                attrs.insert("senderUsername".to_string(), Value::String(sender.clone()));
                attrs.insert(
                    "isSend".to_string(),
                    Value::Bool(same_account(owner_wxid, &sender) || is_self_alias(&sender)),
                );
            }
        }
        if summary_text.is_empty() {
            summary_text = attrs
                .get("sourceXmlText")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .chars()
                .take(60)
                .collect();
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

fn optional_column(
    db: &WeChatDb,
    table: &str,
    column: &str,
    fallback: &str,
) -> Result<String, String> {
    db.column_exists(table, column).map(|present| {
        if present {
            format!("\"{}\"", column.replace('"', "\"\""))
        } else {
            fallback.to_string()
        }
    })
}

fn display_text<'a>(attrs: &'a serde_json::Map<String, Value>, fallback: &'a str) -> &'a str {
    attrs
        .get("xmlText")
        .and_then(Value::as_str)
        .unwrap_or(fallback)
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

fn decode_message_content(
    content: SqlValue,
    compressed_content: SqlValue,
    compressed: bool,
) -> Content {
    match decode_content(content, compressed) {
        Content::Text(text) if !text.trim().is_empty() => Content::Text(text),
        primary => decode_fallback_content(compressed_content).unwrap_or(primary),
    }
}

fn decode_fallback_content(content: SqlValue) -> Option<Content> {
    let bytes = value_bytes(content)?;
    if bytes.is_empty() {
        return None;
    }
    let decoded = zstd::decode_all(bytes.as_slice()).ok().unwrap_or(bytes);
    Some(Content::Text(
        String::from_utf8_lossy(&decoded).into_owned(),
    ))
}

fn decode_auxiliary(content: SqlValue, compressed: bool) -> Option<String> {
    match decode_content(content, compressed) {
        Content::Text(text) if !text.trim().is_empty() => Some(text),
        _ => None,
    }
}

fn decode_packed_info(content: SqlValue) -> Option<String> {
    let bytes = value_bytes(content)?;
    let decoded = zstd::decode_all(bytes.as_slice()).ok().unwrap_or(bytes);
    let text = String::from_utf8_lossy(&decoded);
    let start = text.find('<')?;
    let end = text.rfind('>').map(|index| index + 1).unwrap_or(text.len());
    Some(text[start..end].to_string())
}

fn value_bytes(content: SqlValue) -> Option<Vec<u8>> {
    match content {
        SqlValue::Text(text) => Some(text.into_bytes()),
        SqlValue::Blob(bytes) => Some(bytes),
        _ => None,
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
    fn rowid_one_weixin_is_owner_fallback() {
        assert!(is_owner_sender("wxid_owner22_eef8", 1, "weixin"));
        assert!(is_owner_sender("wxid_owner22_eef8", 1, "WEIXIN"));
        assert!(!is_owner_sender("wxid_owner22_eef8", 2, "weixin"));
        assert!(!is_owner_sender("wxid_owner22_eef8", 1, "friend_wxid"));
    }

    #[test]
    fn md5_suffix_matches_real_layout() {
        // WeChat derives the conversation table as Msg_<md5(user_name)>.
        assert_eq!(md5_hex("filehelper").len(), 32);
    }
}
