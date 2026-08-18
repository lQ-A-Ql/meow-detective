use rusqlite::types::ValueRef;
use serde_json::Value;

use super::CapGuard;
use crate::db::WeChatDb;
use crate::payload::{new_attrs, Payload};

pub fn parse(db: &WeChatDb, payload: &mut Payload) -> Result<usize, String> {
    if !db.table_exists("MessageResourceInfo")? || !db.table_exists("MessageResourceDetail")? {
        return Ok(0);
    }
    let sql = "SELECT i.message_id, i.chat_id, i.sender_id, i.message_local_type, \
               i.message_create_time, i.message_local_id, i.message_svr_id, \
               i.message_origin_source, i.packed_info, d.resource_id, d.type, d.size, \
               d.create_time, d.access_time, d.status, d.data_index, d.packed_info, \
               (SELECT user_name FROM ChatName2Id WHERE rowid = i.chat_id), \
               (SELECT user_name FROM SenderName2Id WHERE rowid = i.sender_id) \
               FROM MessageResourceInfo i \
               JOIN MessageResourceDetail d ON d.message_id = i.message_id \
               ORDER BY i.message_id, d.resource_id";
    let mut statement = db
        .conn()
        .prepare(sql)
        .map_err(|error| format!("message resource query prepare failed: {error}"))?;
    let mut rows = statement
        .query([])
        .map_err(|error| format!("message resource query failed: {error}"))?;
    let mut cap = CapGuard::new();
    let mut emitted = 0usize;
    while let Some(row) = rows
        .next()
        .map_err(|error| format!("message resource row failed: {error}"))?
    {
        if !cap.allow("WeChatMedia", payload) {
            break;
        }
        let mut values = new_attrs();
        for (index, key) in [
            "messageId",
            "chatId",
            "senderId",
            "messageLocalType",
            "messageCreateTime",
            "localId",
            "serverId",
            "messageOriginSource",
        ]
        .into_iter()
        .enumerate()
        {
            insert_integer(row.get_ref(index), key, &mut values);
        }
        insert_integer(row.get_ref(9), "resourceId", &mut values);
        insert_integer(row.get_ref(10), "resourceType", &mut values);
        insert_integer(row.get_ref(11), "sizeBytes", &mut values);
        insert_integer(row.get_ref(12), "resourceCreateTime", &mut values);
        insert_integer(row.get_ref(13), "accessTime", &mut values);
        insert_integer(row.get_ref(14), "status", &mut values);
        insert_text(row.get_ref(15), "dataIndex", &mut values);
        insert_text(row.get_ref(17), "talker", &mut values);
        insert_text(row.get_ref(18), "senderUsername", &mut values);
        if let Some(key) = row
            .get_ref(8)
            .ok()
            .and_then(|value| match value {
                ValueRef::Blob(bytes) => Some(bytes),
                _ => None,
            })
            .and_then(storage_key)
        {
            values.insert("storageKey".to_string(), Value::String(key));
        }
        let mut attrs = new_attrs();
        attrs.insert(
            "table".to_string(),
            Value::String("MessageResourceDetail".to_string()),
        );
        attrs.insert("values".to_string(), Value::Object(values));
        let message_id = row.get::<_, i64>(0).unwrap_or_default();
        let resource_id = row.get::<_, i64>(9).unwrap_or_default();
        payload.artifact(
            "WeChatMedia",
            format!("消息资源 {message_id}/{resource_id}"),
            "微信消息与本地媒体资源的确定性关联记录",
            attrs,
        );
        emitted += 1;
    }
    Ok(emitted)
}

fn insert_integer(
    value: Result<ValueRef<'_>, rusqlite::Error>,
    key: &str,
    values: &mut serde_json::Map<String, Value>,
) {
    if let Ok(ValueRef::Integer(number)) = value {
        values.insert(key.to_string(), Value::from(number));
    }
}

fn insert_text(
    value: Result<ValueRef<'_>, rusqlite::Error>,
    key: &str,
    values: &mut serde_json::Map<String, Value>,
) {
    if let Ok(ValueRef::Text(text)) = value {
        let text = String::from_utf8_lossy(text).trim().to_string();
        if !text.is_empty() {
            values.insert(key.to_string(), Value::String(text));
        }
    }
}

fn storage_key(bytes: &[u8]) -> Option<String> {
    bytes.windows(32).find_map(|window| {
        window.iter().all(u8::is_ascii_hexdigit).then(|| {
            String::from_utf8_lossy(window)
                .to_ascii_lowercase()
                .to_string()
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_storage_key_from_packed_resource_metadata() {
        let bytes = b"\x12\x2283d35dbfebf20beff6c1e711168205ee\0";
        assert_eq!(
            storage_key(bytes).as_deref(),
            Some("83d35dbfebf20beff6c1e711168205ee")
        );
    }
}
