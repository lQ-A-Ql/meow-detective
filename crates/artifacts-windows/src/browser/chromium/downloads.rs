use super::sqlite::{open_sqlite_from_bytes, row_get_opt};
use super::time::webkit_time_to_dt;
use super::types::BrowserDownload;
use rusqlite::params;
use std::collections::HashMap;

/// Parse a Chromium `History` SQLite database for download records.
///
/// Reads the `downloads` table and, when available, the `downloads_url_chains`
/// table that holds the download URL in newer Chromium builds.
pub fn parse_chrome_downloads(
    data: &[u8],
    browser: &str,
    profile: Option<&str>,
) -> Result<Vec<BrowserDownload>, String> {
    let (conn, _tmp) = open_sqlite_from_bytes(data)?;

    let mut url_map = HashMap::<i64, String>::new();
    if let Ok(mut stmt) =
        conn.prepare("SELECT id, url FROM downloads_url_chains WHERE chain_index = 0")
    {
        if let Ok(rows) = stmt.query_map(params![], |row| {
            let id: i64 = row.get(0)?;
            let url: String = row.get(1)?;
            Ok((id, url))
        }) {
            for row in rows.flatten() {
                url_map.insert(row.0, row.1);
            }
        }
    }

    let mut stmt = if let Ok(statement) = conn.prepare(
        "SELECT id, target_path, start_time, end_time, total_bytes, tab_url
         FROM downloads
         ORDER BY start_time DESC",
    ) {
        statement
    } else {
        conn.prepare(
            "SELECT id, target_path, start_time, total_bytes
             FROM downloads
             ORDER BY start_time DESC",
        )
        .map_err(|e| format!("prepare downloads query: {}", e))?
    };

    let rows = stmt
        .query_map(params![], |row| {
            let id: i64 = row.get(0)?;
            let target_path: Option<String> = row.get(1).ok();
            let start_time_raw: i64 = row.get(2).unwrap_or(0);
            let end_time_raw: Option<i64> = row_get_opt(row, "end_time");
            let total_bytes: i64 = row_get_opt(row, "total_bytes").unwrap_or(0);
            let tab_url: Option<String> = row_get_opt(row, "tab_url");
            let url = url_map.remove(&id).or(tab_url).unwrap_or_default();

            Ok(BrowserDownload {
                url,
                target_path,
                start_time: webkit_time_to_dt(start_time_raw),
                end_time: end_time_raw.and_then(webkit_time_to_dt),
                total_bytes,
                browser: browser.to_string(),
                profile: profile.map(|s| s.to_string()),
            })
        })
        .map_err(|e| format!("query downloads: {}", e))?;

    let mut results = Vec::new();
    for row in rows {
        match row {
            Ok(download) => results.push(download),
            Err(e) => tracing::warn!("skipping download row: {}", e),
        }
    }

    Ok(results)
}
