//! `sns.db` → WeChatMoment: one artifact per row of `SnsTimeLine`.
//!
//! Schema: `SnsTimeLine(tid PK, user_name, content TEXT, pack_info_buf
//! TEXT)`; `content` carries the post/media XML and `pack_info_buf` carries
//! interaction metadata. Parsing is intentionally tolerant of incomplete XML.

use serde_json::Value;

use super::{insert_text, unix_to_rfc3339, xml, CapGuard};
use crate::db::WeChatDb;
use crate::payload::{new_attrs, Payload};

/// Parse `SnsTimeLine` into WeChatMoment artifacts.
pub fn parse(db: &WeChatDb, payload: &mut Payload) -> Result<usize, String> {
    if !db.table_exists("SnsTimeLine")? {
        return Ok(0);
    }
    let pack_column = if db.column_exists("SnsTimeLine", "pack_info_buf")? {
        "pack_info_buf"
    } else {
        "''"
    };
    let mut stmt = db
        .conn()
        .prepare(&format!(
            "SELECT tid, user_name, content, {pack_column} FROM SnsTimeLine ORDER BY tid"
        ))
        .map_err(|error| format!("SnsTimeLine query prepare failed: {error}"))?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0).unwrap_or_default(),
                row.get::<_, String>(1).unwrap_or_default(),
                row.get::<_, String>(2).unwrap_or_default(),
                row.get::<_, String>(3).unwrap_or_default(),
            ))
        })
        .map_err(|error| format!("SnsTimeLine query failed: {error}"))?;

    let mut cap = CapGuard::new();
    let mut emitted = 0usize;
    for row in rows {
        let (tid, user_name, content, pack_info) =
            row.map_err(|error| format!("SnsTimeLine row failed: {error}"))?;
        if !cap.allow("WeChatMoment", payload) {
            break;
        }
        let mut attrs = new_attrs();
        attrs.insert("tid".to_string(), Value::from(tid));
        let author = xml::tag_text(&content, "username").unwrap_or(user_name);
        insert_text(&mut attrs, "userName", &author);
        if let Some(desc) = xml::tag_text(&content, "contentDesc") {
            insert_text(&mut attrs, "contentDesc", &desc);
        }
        if let Some(id) = xml::tag_text(&content, "id") {
            insert_text(&mut attrs, "snsId", &id);
        }
        let created = xml::tag_text(&content, "createTime")
            .and_then(|text| text.parse::<i64>().ok())
            .and_then(unix_to_rfc3339);
        if let Some(ts) = &created {
            attrs.insert("createTimeUtc".to_string(), Value::String(ts.clone()));
        }
        let media_items = xml::media_items(&content);
        let media = !media_items.is_empty();
        attrs.insert("hasMedia".to_string(), Value::Bool(media));
        attrs.insert(
            "mediaCount".to_string(),
            Value::from(media_items.len() as u64),
        );
        if media {
            attrs.insert("mediaItems".to_string(), Value::Array(media_items));
        }
        let mut likes = xml::interaction_items(&pack_info, "likeUser");
        if likes.is_empty() {
            likes = xml::interaction_items(&pack_info, "like");
        }
        let mut comments = xml::interaction_items(&pack_info, "commentUser");
        if comments.is_empty() {
            comments = xml::interaction_items(&pack_info, "comment");
        }
        attrs.insert("likeCount".to_string(), Value::from(likes.len() as u64));
        attrs.insert(
            "commentCount".to_string(),
            Value::from(comments.len() as u64),
        );
        if !likes.is_empty() {
            attrs.insert("likes".to_string(), Value::Array(likes));
        }
        if !comments.is_empty() {
            attrs.insert("comments".to_string(), Value::Array(comments));
        }
        let shown = if author.trim().is_empty() {
            "<unknown>".to_string()
        } else {
            author.trim().to_string()
        };
        payload.artifact(
            "WeChatMoment",
            format!("朋友圈 {shown} tid={tid}"),
            format!(
                "微信朋友圈动态（{shown}，{}）",
                if media {
                    "含媒体附件"
                } else {
                    "纯文本/无媒体"
                }
            ),
            attrs,
        );
        if let Some(ts) = created {
            let mut event_attrs = new_attrs();
            event_attrs.insert("tid".to_string(), Value::from(tid));
            insert_text(&mut event_attrs, "userName", &shown);
            payload.timeline_event(
                ts,
                "WeChatMoment",
                format!("{shown} 发布朋友圈"),
                event_attrs,
            );
        }
        emitted += 1;
    }
    Ok(emitted)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tag_scan_tolerates_messy_content() {
        let xml = "<SnsDataItem><TimelineObject><id>123</id><username>wxid_a</username>\
            <createTime>1774857283</createTime><contentDesc>hello</contentDesc>\
            <mediaList><media id=\"1\"/></mediaList>";
        assert_eq!(xml::tag_text(xml, "id").as_deref(), Some("123"));
        assert_eq!(xml::tag_text(xml, "username").as_deref(), Some("wxid_a"));
        assert_eq!(xml::tag_text(xml, "contentDesc").as_deref(), Some("hello"));
        assert_eq!(xml::media_items(xml).len(), 1);
        assert_eq!(xml::tag_text(xml, "missing"), None);
    }

    #[test]
    fn empty_or_self_closed_media_list_is_no_media() {
        assert!(xml::media_items("<mediaList/>").is_empty());
        assert!(xml::media_items("<mediaList></mediaList>").is_empty());
        assert!(xml::media_items("no list at all").is_empty());
    }
}
