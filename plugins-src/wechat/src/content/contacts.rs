//! `contact.db` → WeChatContact: one artifact per row of `contact`.
//!
//! Schema (WeChat 4.x, verified on 4.1.8.67): `contact(id, username,
//! local_type, alias, encrypt_username, flag, delete_flag, verify_flag,
//! remark, ..., nick_name, ..., big_head_url, small_head_url, head_img_md5,
//! ..., description, extra_buffer BLOB, chat_room_type)`. `encrypt_username`
//! is emitted verbatim when present (best effort, never decoded).
//! `delete_flag = 1` marks the contact as `deleted: true`.

use serde_json::Value;

use super::{insert_text, CapGuard};
use crate::db::WeChatDb;
use crate::payload::{new_attrs, Payload};

/// Parse the `contact` table into WeChatContact artifacts. Returns the
/// number of artifacts emitted; a missing/unreadable table is a warning,
/// not an error.
pub fn parse(db: &WeChatDb, payload: &mut Payload) -> Result<usize, String> {
    if !db.table_exists("contact")? {
        return Ok(0);
    }
    let mut stmt = db
        .conn()
        .prepare(
            "SELECT username, local_type, alias, encrypt_username, delete_flag, \
             remark, nick_name, head_img_md5, description FROM contact",
        )
        .map_err(|error| format!("contact query prepare failed: {error}"))?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0).unwrap_or_default(),
                row.get::<_, i64>(1).unwrap_or_default(),
                row.get::<_, String>(2).unwrap_or_default(),
                row.get::<_, String>(3).unwrap_or_default(),
                row.get::<_, i64>(4).unwrap_or_default(),
                row.get::<_, String>(5).unwrap_or_default(),
                row.get::<_, String>(6).unwrap_or_default(),
                row.get::<_, String>(7).unwrap_or_default(),
                row.get::<_, String>(8).unwrap_or_default(),
            ))
        })
        .map_err(|error| format!("contact query failed: {error}"))?;

    let mut cap = CapGuard::new();
    let mut emitted = 0usize;
    for row in rows {
        let (
            username,
            local_type,
            alias,
            encrypt_username,
            delete_flag,
            remark,
            nick,
            head_md5,
            desc,
        ) = row.map_err(|error| format!("contact row failed: {error}"))?;
        if !cap.allow("WeChatContact", payload) {
            break;
        }
        let mut attrs = new_attrs();
        attrs.insert("username".to_string(), Value::String(username.clone()));
        attrs.insert("localType".to_string(), Value::from(local_type));
        insert_text(&mut attrs, "alias", &alias);
        insert_text(&mut attrs, "nickName", &nick);
        insert_text(&mut attrs, "remark", &remark);
        insert_text(&mut attrs, "encryptUsername", &encrypt_username);
        insert_text(&mut attrs, "headImgMd5", &head_md5);
        insert_text(&mut attrs, "description", &desc);
        if delete_flag == 1 {
            attrs.insert("deleted".to_string(), Value::Bool(true));
        }
        let display = if !remark.trim().is_empty() {
            remark.trim().to_string()
        } else if !nick.trim().is_empty() {
            nick.trim().to_string()
        } else {
            username.clone()
        };
        payload.artifact(
            "WeChatContact",
            format!("联系人 {display}"),
            format!("微信联系人 {display}（{username}）"),
            attrs,
        );
        emitted += 1;
    }
    Ok(emitted)
}
