use base64::Engine as _;
use rusqlite::types::ValueRef;
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

use super::CapGuard;
use crate::db::WeChatDb;
use crate::payload::{new_attrs, Payload};

const INLINE_MEDIA_BYTES: usize = 2 * 1024 * 1024;
const INLINE_MEDIA_TOTAL_BYTES: usize = 32 * 1024 * 1024;
const MEDIA_SIGNATURE_SCAN_BYTES: usize = 4 * 1024;

struct InlineMediaBudget {
    remaining: usize,
}

impl InlineMediaBudget {
    fn new() -> Self {
        Self {
            remaining: INLINE_MEDIA_TOTAL_BYTES,
        }
    }

    fn consume(&mut self, bytes: usize) -> bool {
        if bytes > INLINE_MEDIA_BYTES || bytes > self.remaining {
            return false;
        }
        self.remaining -= bytes;
        true
    }
}

pub fn parse_resource(db: &WeChatDb, payload: &mut Payload) -> Result<usize, String> {
    parse_tables(db, "WeChatMedia", payload, |_| true)
}

pub fn parse_fts(db: &WeChatDb, payload: &mut Payload) -> Result<usize, String> {
    parse_tables(db, "WeChatSearchRecord", payload, |table| {
        table.ends_with("_content") || !table.contains('_')
    })
}

fn parse_tables(
    db: &WeChatDb,
    family: &str,
    payload: &mut Payload,
    include: impl Fn(&str) -> bool,
) -> Result<usize, String> {
    let tables = db.table_list()?;
    let mut cap = CapGuard::new();
    let mut inline_budget = InlineMediaBudget::new();
    let mut emitted = 0usize;
    for table in tables.into_iter().filter(|table| include(table)) {
        match parse_table(db, &table, family, payload, &mut cap, &mut inline_budget) {
            Ok(count) => emitted += count,
            Err(reason) => payload.warn(format!("{table} 补充内容解析跳过：{reason}")),
        }
    }
    Ok(emitted)
}

fn parse_table(
    db: &WeChatDb,
    table: &str,
    family: &str,
    payload: &mut Payload,
    cap: &mut CapGuard,
    inline_budget: &mut InlineMediaBudget,
) -> Result<usize, String> {
    let escaped = table.replace('"', "\"\"");
    let mut statement = db
        .conn()
        .prepare(&format!("SELECT * FROM \"{escaped}\""))
        .map_err(|error| format!("query prepare failed: {error}"))?;
    let columns = statement
        .column_names()
        .into_iter()
        .map(str::to_string)
        .collect::<Vec<_>>();
    let mut rows = statement
        .query([])
        .map_err(|error| format!("query failed: {error}"))?;
    let mut emitted = 0usize;
    while let Some(row) = rows
        .next()
        .map_err(|error| format!("row iteration failed: {error}"))?
    {
        if !cap.allow(family, payload) {
            break;
        }
        let mut attrs = new_attrs();
        attrs.insert("table".to_string(), Value::String(table.to_string()));
        attrs.insert("rowIndex".to_string(), Value::from(emitted as u64));
        let mut values = Map::new();
        for (index, column) in columns.iter().enumerate() {
            let value = row
                .get_ref(index)
                .map_err(|error| format!("column {column} failed: {error}"))?;
            if let Some(projected) = project_value(value, inline_budget) {
                values.insert(column.clone(), projected);
            }
        }
        attrs.insert("values".to_string(), Value::Object(values));
        payload.artifact(
            family,
            format!("{table} #{}", emitted + 1),
            if family == "WeChatMedia" {
                "微信媒体资源记录".to_string()
            } else {
                "微信全文索引补充记录".to_string()
            },
            attrs,
        );
        emitted += 1;
    }
    Ok(emitted)
}

fn project_value(value: ValueRef<'_>, inline_budget: &mut InlineMediaBudget) -> Option<Value> {
    match value {
        ValueRef::Null => None,
        ValueRef::Integer(value) => Some(Value::from(value)),
        ValueRef::Real(value) => serde_json::Number::from_f64(value).map(Value::Number),
        ValueRef::Text(value) => Some(Value::String(String::from_utf8_lossy(value).into_owned())),
        ValueRef::Blob(value) => Some(blob_value(value, inline_budget)),
    }
}

fn blob_value(bytes: &[u8], inline_budget: &mut InlineMediaBudget) -> Value {
    let mut object = Map::new();
    object.insert("sizeBytes".to_string(), Value::from(bytes.len() as u64));
    object.insert(
        "sha256".to_string(),
        Value::String(format!("{:x}", Sha256::digest(bytes))),
    );
    if let Some((mime, offset)) = media_payload(bytes) {
        let payload = &bytes[offset..];
        object.insert("mimeType".to_string(), Value::String(mime.to_string()));
        object.insert("mediaOffset".to_string(), Value::from(offset as u64));
        object.insert(
            "mediaSizeBytes".to_string(),
            Value::from(payload.len() as u64),
        );
        if inline_budget.consume(payload.len()) {
            object.insert(
                "inlineDataBase64".to_string(),
                Value::String(base64::engine::general_purpose::STANDARD.encode(payload)),
            );
        } else {
            object.insert(
                "inlineOmitted".to_string(),
                Value::String("media size or database inline budget exceeded".to_string()),
            );
        }
    }
    Value::Object(object)
}

pub(super) fn inline_media_value(bytes: &[u8]) -> Value {
    blob_value(bytes, &mut InlineMediaBudget::new())
}

fn media_payload(bytes: &[u8]) -> Option<(&'static str, usize)> {
    if let Some(mime) = media_mime(bytes) {
        return Some((mime, 0));
    }
    let limit = bytes.len().min(MEDIA_SIGNATURE_SCAN_BYTES);
    (1..limit).find_map(|offset| media_mime(&bytes[offset..]).map(|mime| (mime, offset)))
}

fn media_mime(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if bytes.starts_with(b"\xff\xd8\xff") {
        Some("image/jpeg")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some("image/gif")
    } else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        Some("image/webp")
    } else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WAVE") {
        Some("audio/wav")
    } else if bytes.starts_with(b"OggS") {
        Some("audio/ogg")
    } else if bytes.starts_with(b"#!SILK_V3") {
        Some("audio/silk")
    } else if bytes.starts_with(b"#!AMR") {
        Some("audio/amr")
    } else if bytes.starts_with(b"ID3") || bytes.starts_with(&[0xff, 0xfb]) {
        Some("audio/mpeg")
    } else if bytes
        .get(4..12)
        .is_some_and(|brand| matches!(brand, b"ftypheic" | b"ftypheix" | b"ftypmif1"))
    {
        Some("image/heic")
    } else if bytes.get(4..8) == Some(b"ftyp") {
        Some("video/mp4")
    } else if bytes.starts_with(b"wxgf") {
        Some("application/x-wechat-wxgf")
    } else if bytes.starts_with(b"wxam") {
        Some("application/x-wechat-wxam")
    } else if is_svg(bytes) {
        Some("image/svg+xml")
    } else {
        None
    }
}

fn is_svg(bytes: &[u8]) -> bool {
    let sample = &bytes[..bytes.len().min(MEDIA_SIGNATURE_SCAN_BYTES)];
    let Ok(text) = std::str::from_utf8(sample) else {
        return false;
    };
    let trimmed = text.trim_start_matches(|character: char| character.is_whitespace());
    trimmed.starts_with("<svg")
        || (trimmed.starts_with("<?xml") && trimmed.to_ascii_lowercase().contains("<svg"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_image_blob_is_inline_and_hashed() {
        let value = blob_value(b"\x89PNG\r\n\x1a\nbody", &mut InlineMediaBudget::new());
        assert_eq!(value["mimeType"], "image/png");
        assert!(value["inlineDataBase64"].as_str().is_some());
        assert_eq!(value["sha256"].as_str().map(str::len), Some(64));
    }

    #[test]
    fn embedded_image_payload_skips_resource_wrapper() {
        let value = blob_value(
            b"wrapper\0\x89PNG\r\n\x1a\nbody",
            &mut InlineMediaBudget::new(),
        );
        assert_eq!(value["mimeType"], "image/png");
        assert_eq!(value["mediaOffset"], 8);
        assert_eq!(
            value["inlineDataBase64"],
            base64::engine::general_purpose::STANDARD.encode(b"\x89PNG\r\n\x1a\nbody")
        );
    }

    #[test]
    fn svg_payload_is_detected_without_dom_execution() {
        let value = blob_value(
            br#"<?xml version="1.0"?><svg xmlns="http://www.w3.org/2000/svg"/>"#,
            &mut InlineMediaBudget::new(),
        );
        assert_eq!(value["mimeType"], "image/svg+xml");
        assert!(value["inlineDataBase64"].as_str().is_some());
    }
}
