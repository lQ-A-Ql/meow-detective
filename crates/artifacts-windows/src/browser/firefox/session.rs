use super::time::unix_millis_to_dt;
use crate::browser::chromium::BrowserSessionTab;
use chrono::{DateTime, Utc};
use serde_json::Value;

pub fn parse_firefox_session(data: &[u8]) -> Result<Vec<BrowserSessionTab>, String> {
    let json_bytes = if data.len() >= 8 && &data[..8] == b"mozLz40\0" {
        decompress_mozlz4(data)?
    } else {
        data.to_vec()
    };
    let text = std::str::from_utf8(&json_bytes)
        .map_err(|e| format!("session file is not valid UTF-8: {}", e))?;
    let root: Value =
        serde_json::from_str(text).map_err(|e| format!("session JSON parse error: {}", e))?;
    let windows = match root.get("windows") {
        Some(Value::Array(windows)) => windows,
        _ => return Ok(Vec::new()),
    };

    let mut results = Vec::new();
    for (window_position, window) in windows.iter().enumerate() {
        parse_window(window, window_position, &mut results);
    }
    Ok(results)
}

fn parse_window(window: &Value, position: usize, results: &mut Vec<BrowserSessionTab>) {
    let window_index = window
        .get("index")
        .and_then(Value::as_i64)
        .unwrap_or(position as i64) as i32;
    let Some(tabs) = window.get("tabs").and_then(Value::as_array) else {
        return;
    };
    for (tab_position, tab) in tabs.iter().enumerate() {
        parse_tab(tab, window_index, tab_position as i32, results);
    }
}

fn parse_tab(tab: &Value, window_index: i32, tab_index: i32, results: &mut Vec<BrowserSessionTab>) {
    let last_active = tab
        .get("lastAccessed")
        .and_then(Value::as_i64)
        .and_then(unix_millis_to_dt);
    let Some(entries) = tab.get("entries").and_then(Value::as_array) else {
        if let Some(result) = parse_session_tab_entry(tab, window_index, tab_index, last_active) {
            results.push(result);
        }
        return;
    };
    let active_index = tab
        .get("index")
        .and_then(Value::as_i64)
        .map(|index| (index - 1).max(0) as usize)
        .unwrap_or(0);
    if let Some(result) = entries
        .get(active_index)
        .and_then(|entry| parse_session_tab_entry(entry, window_index, tab_index, last_active))
    {
        results.push(result);
        return;
    }
    if let Some(result) = entries
        .iter()
        .find_map(|entry| parse_session_tab_entry(entry, window_index, tab_index, last_active))
    {
        results.push(result);
    }
}

fn parse_session_tab_entry(
    entry: &Value,
    window_index: i32,
    tab_index: i32,
    last_active: Option<DateTime<Utc>>,
) -> Option<BrowserSessionTab> {
    let url = entry
        .get("url")
        .and_then(Value::as_str)
        .filter(|url| !url.is_empty())?;
    let title = entry
        .get("title")
        .and_then(Value::as_str)
        .filter(|title| !title.is_empty())
        .map(str::to_string);
    Some(BrowserSessionTab {
        url: url.to_string(),
        title,
        window_index,
        tab_index,
        last_active,
    })
}

pub(super) fn decompress_mozlz4(data: &[u8]) -> Result<Vec<u8>, String> {
    if data.len() < 12 {
        return Err("data too short for mozLz4 header".to_string());
    }
    if &data[..8] != b"mozLz40\0" {
        return Err("not a mozLz4 stream".to_string());
    }
    let size = u32::from_le_bytes([data[8], data[9], data[10], data[11]]) as usize;
    if size > 512 * 1024 * 1024 {
        return Err(format!(
            "mozLz4 uncompressed size {} exceeds safety limit",
            size
        ));
    }
    let mut decompressed = vec![0; size];
    lz4_flex::block::decompress_into(&data[12..], &mut decompressed)
        .map_err(|e| format!("lz4 decompress: {}", e))?;
    Ok(decompressed)
}
