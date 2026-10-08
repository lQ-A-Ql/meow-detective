//! Bounded parser for Firefox `extensions.json`.
//!
//! The file is a JSON registry written by Firefox.  Add-on records have
//! changed shape across Firefox releases, so this parser deliberately reads
//! only the stable forensic fields and treats unknown fields as opaque.

use chrono::{DateTime, TimeZone, Utc};
use serde_json::Value;

const MAX_EXTENSIONS_JSON_BYTES: usize = 16 * 1024 * 1024;

/// Metadata for one Firefox add-on.  No extension payload or executable code
/// is loaded by this parser.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserExtension {
    pub id: String,
    pub name: String,
    pub version: String,
    pub active: bool,
    pub user_disabled: bool,
    pub install_date: Option<DateTime<Utc>>,
    pub update_date: Option<DateTime<Utc>>,
    /// Firefox's signedState value.  It is kept as text because older
    /// versions emit a number while newer builds may emit a symbolic value.
    pub signed_state: Option<String>,
    pub permissions: Vec<String>,
}

/// Parse a Firefox `extensions.json` registry with a strict input bound.
pub fn parse_firefox_extensions(data: &[u8]) -> Result<Vec<BrowserExtension>, String> {
    if data.len() > MAX_EXTENSIONS_JSON_BYTES {
        return Err(format!(
            "firefox extensions.json exceeds {} byte limit",
            MAX_EXTENSIONS_JSON_BYTES
        ));
    }
    let root: Value = serde_json::from_slice(data)
        .map_err(|error| format!("parse firefox extensions.json: {error}"))?;
    let addons = root
        .get("addons")
        .and_then(Value::as_array)
        .ok_or_else(|| "firefox extensions.json has no addons array".to_string())?;

    let mut extensions = Vec::with_capacity(addons.len().min(1024));
    for addon in addons {
        let Some(object) = addon.as_object() else {
            continue;
        };
        let id = non_empty_string(object.get("id"));
        if id.is_empty() {
            continue;
        }
        let name = addon_name(object);
        let version = string_value(object.get("version"));
        let active = object
            .get("active")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let user_disabled = object
            .get("userDisabled")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let permissions = object
            .get("permissions")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|value| value.as_str().map(str::to_string))
                    .filter(|permission| !permission.is_empty())
                    .take(512)
                    .collect()
            })
            .unwrap_or_default();
        extensions.push(BrowserExtension {
            id,
            name,
            version,
            active,
            user_disabled,
            install_date: date_value(object.get("installDate")),
            update_date: date_value(object.get("updateDate")),
            signed_state: object
                .get("signedState")
                .map(|value| string_value(Some(value))),
            permissions,
        });
    }
    Ok(extensions)
}

fn addon_name(object: &serde_json::Map<String, Value>) -> String {
    if let Some(name) = object.get("name").and_then(Value::as_str) {
        if !name.is_empty() {
            return name.to_string();
        }
    }
    object
        .get("defaultLocale")
        .and_then(Value::as_object)
        .and_then(|locale| locale.get("name"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn non_empty_string(value: Option<&Value>) -> String {
    value
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string()
}

fn string_value(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(value)) => value.clone(),
        Some(Value::Number(value)) => value.to_string(),
        Some(Value::Bool(value)) => value.to_string(),
        _ => String::new(),
    }
}

fn date_value(value: Option<&Value>) -> Option<DateTime<Utc>> {
    let millis = value.and_then(|value| {
        value
            .as_i64()
            .or_else(|| value.as_u64().and_then(|number| i64::try_from(number).ok()))
    })?;
    if millis <= 0 {
        return None;
    }
    Utc.timestamp_millis_opt(millis).single()
}
