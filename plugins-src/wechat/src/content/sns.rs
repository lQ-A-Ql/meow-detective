//! `sns.db` → WeChatMoment: one artifact per row of `SnsTimeLine`.
//!
//! Schema: `SnsTimeLine(tid PK, user_name, content TEXT, pack_info_buf
//! TEXT)`; `content` is a `<SnsDataItem><TimelineObject>...` XML document.
//! Real-world content is not always well-formed, so extraction is a
//! tolerant hand-rolled tag scan (first `<tag>...</tag>` text wins) rather
//! than a strict XML parser.

use serde_json::Value;

use super::{insert_text, unix_to_rfc3339, CapGuard};
use crate::db::WeChatDb;
use crate::payload::{new_attrs, Payload};

/// First `<tag>...</tag>` inner text, or `None` when absent/unclosed.
fn tag_text<'a>(xml: &'a str, tag: &str) -> Option<&'a str> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = xml.find(&open)? + open.len();
    let end = xml[start..].find(&close)? + start;
    Some(xml[start..end].trim())
}

/// A non-empty `<mediaList>` (contains at least one `<media` element) means
/// the moment carries attachments.
fn has_media(xml: &str) -> bool {
    let Some(start) = xml.find("<mediaList>").map(|i| i + "<mediaList>".len()) else {
        return false;
    };
    let end = xml[start..]
        .find("</mediaList>")
        .map(|i| i + start)
        .unwrap_or(xml.len());
    xml[start..end].contains("<media")
}

/// Parse `SnsTimeLine` into WeChatMoment artifacts.
pub fn parse(db: &WeChatDb, payload: &mut Payload) -> Result<usize, String> {
    if !db.table_exists("SnsTimeLine")? {
        return Ok(0);
    }
    let mut stmt = db
        .conn()
        .prepare("SELECT tid, user_name, content FROM SnsTimeLine ORDER BY tid")
        .map_err(|error| format!("SnsTimeLine query prepare failed: {error}"))?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0).unwrap_or_default(),
                row.get::<_, String>(1).unwrap_or_default(),
                row.get::<_, String>(2).unwrap_or_default(),
            ))
        })
        .map_err(|error| format!("SnsTimeLine query failed: {error}"))?;

    let mut cap = CapGuard::new();
    let mut emitted = 0usize;
    for row in rows {
        let (tid, user_name, content) =
            row.map_err(|error| format!("SnsTimeLine row failed: {error}"))?;
        if !cap.allow("WeChatMoment", payload) {
            break;
        }
        let mut attrs = new_attrs();
        attrs.insert("tid".to_string(), Value::from(tid));
        let author = tag_text(&content, "username").unwrap_or(user_name.as_str());
        insert_text(&mut attrs, "userName", author);
        if let Some(desc) = tag_text(&content, "contentDesc") {
            insert_text(&mut attrs, "contentDesc", desc);
        }
        if let Some(id) = tag_text(&content, "id") {
            insert_text(&mut attrs, "snsId", id);
        }
        let created = tag_text(&content, "createTime")
            .and_then(|text| text.parse::<i64>().ok())
            .and_then(unix_to_rfc3339);
        if let Some(ts) = &created {
            attrs.insert("createTimeUtc".to_string(), Value::String(ts.clone()));
        }
        let media = has_media(&content);
        attrs.insert("hasMedia".to_string(), Value::Bool(media));
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
        assert_eq!(tag_text(xml, "id"), Some("123"));
        assert_eq!(tag_text(xml, "username"), Some("wxid_a"));
        assert_eq!(tag_text(xml, "contentDesc"), Some("hello"));
        assert!(has_media(xml));
        assert_eq!(tag_text(xml, "missing"), None);
    }

    #[test]
    fn empty_or_self_closed_media_list_is_no_media() {
        assert!(!has_media("<mediaList/>"));
        assert!(!has_media("<mediaList></mediaList>"));
        assert!(!has_media("no list at all"));
    }
}
