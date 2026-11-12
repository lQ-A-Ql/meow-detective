use serde_json::Value;
use std::collections::BTreeMap;
use transport::dto::{EmailAttachmentDto, EmailHeaderDto};

pub(super) fn string_attr(attrs: &BTreeMap<String, Value>, key: &str) -> String {
    attrs
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_default()
}

pub(super) fn optional_string_attr(attrs: &BTreeMap<String, Value>, key: &str) -> Option<String> {
    attrs
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

pub(super) fn u64_attr(attrs: &BTreeMap<String, Value>, key: &str) -> u64 {
    attrs.get(key).and_then(Value::as_u64).unwrap_or(0)
}

pub(super) fn bool_attr(attrs: &BTreeMap<String, Value>, key: &str) -> bool {
    attrs.get(key).and_then(Value::as_bool).unwrap_or(false)
}

pub(super) fn optional_u32_attr(attrs: &BTreeMap<String, Value>, key: &str) -> Option<u32> {
    attrs.get(key).and_then(Value::as_u64).map(|v| v as u32)
}

pub(super) fn optional_u64_attr(attrs: &BTreeMap<String, Value>, key: &str) -> Option<u64> {
    attrs.get(key).and_then(Value::as_u64)
}

pub(super) fn details_attr(attrs: &BTreeMap<String, Value>, key: &str) -> BTreeMap<String, String> {
    attrs
        .get(key)
        .and_then(|value| {
            if let Some(obj) = value.as_object() {
                Some(
                    obj.iter()
                        .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                        .collect(),
                )
            } else {
                serde_json::from_value::<BTreeMap<String, String>>(value.clone()).ok()
            }
        })
        .unwrap_or_default()
}

pub(super) fn optional_i64_attr(attrs: &BTreeMap<String, Value>, key: &str) -> Option<i64> {
    attrs.get(key).and_then(Value::as_i64)
}

pub(super) fn i32_attr(attrs: &BTreeMap<String, Value>, key: &str) -> i32 {
    attrs.get(key).and_then(Value::as_i64).unwrap_or(0) as i32
}

pub(super) fn u32_attr(attrs: &BTreeMap<String, Value>, key: &str) -> u32 {
    attrs.get(key).and_then(Value::as_u64).unwrap_or(0) as u32
}

pub(super) fn optional_bool_attr(attrs: &BTreeMap<String, Value>, key: &str) -> Option<bool> {
    attrs.get(key).and_then(Value::as_bool)
}

pub(super) fn string_vec_attr(attrs: &BTreeMap<String, Value>, key: &str) -> Vec<String> {
    attrs
        .get(key)
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

pub(super) fn attachment_details_attr(
    attrs: &BTreeMap<String, Value>,
    key: &str,
) -> Vec<EmailAttachmentDto> {
    attrs
        .get(key)
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(|v| {
                    Some(EmailAttachmentDto {
                        file_name: v.get("fileName")?.as_str()?.to_string(),
                        size: v.get("size")?.as_u64(),
                        mime_type: v.get("mimeType")?.as_str().map(str::to_string),
                        content_id: v.get("contentId")?.as_str().map(str::to_string),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

pub(super) fn header_attr(attrs: &BTreeMap<String, Value>, key: &str) -> Vec<EmailHeaderDto> {
    attrs
        .get(key)
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(|v| {
                    Some(EmailHeaderDto {
                        name: v.get("name")?.as_str()?.to_string(),
                        value: v.get("value")?.as_str()?.to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}
