use super::time::webkit_time_to_dt;
use super::types::BrowserSessionTab;

/// Parse a Chromium session restore JSON file.
pub fn parse_chrome_session(data: &[u8]) -> Result<Vec<BrowserSessionTab>, String> {
    let text =
        std::str::from_utf8(data).map_err(|e| format!("session file is not valid UTF-8: {}", e))?;
    let root: serde_json::Value =
        serde_json::from_str(text).map_err(|e| format!("session JSON parse error: {}", e))?;

    let windows = match root.get("windows") {
        Some(serde_json::Value::Array(windows)) => windows,
        _ => return Ok(parse_top_level_tabs(&root)),
    };

    let mut results = Vec::new();
    for window in windows {
        let window_index = window
            .get("index")
            .and_then(|value| value.as_i64())
            .unwrap_or(0) as i32;
        let Some(serde_json::Value::Array(tabs)) = window.get("tabs") else {
            continue;
        };

        for (fallback_index, tab) in tabs.iter().enumerate() {
            let tab_index = tab
                .get("index")
                .and_then(|value| value.as_i64())
                .unwrap_or(fallback_index as i64) as i32;
            if let Some(result) = parse_session_tab_entry(tab, window_index, tab_index) {
                results.push(result);
            }
        }
    }

    Ok(results)
}

fn parse_top_level_tabs(root: &serde_json::Value) -> Vec<BrowserSessionTab> {
    root.as_array()
        .into_iter()
        .flatten()
        .enumerate()
        .filter_map(|(index, tab)| parse_session_tab_entry(tab, 0, index as i32))
        .collect()
}

fn parse_session_tab_entry(
    tab: &serde_json::Value,
    window_index: i32,
    tab_index: i32,
) -> Option<BrowserSessionTab> {
    let url = tab
        .get("url")
        .and_then(|value| value.as_str())
        .filter(|value| !value.is_empty())?;
    let title = tab
        .get("title")
        .and_then(|value| value.as_str())
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let last_active = tab
        .get("last_active_time")
        .or_else(|| tab.get("last_navigation_time"))
        .or_else(|| tab.get("timestamp"))
        .and_then(|value| value.as_i64())
        .and_then(webkit_time_to_dt);

    Some(BrowserSessionTab {
        url: url.to_string(),
        title,
        window_index,
        tab_index,
        last_active,
    })
}
