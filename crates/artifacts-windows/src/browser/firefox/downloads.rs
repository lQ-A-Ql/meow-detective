use super::sqlite::{open_sqlite_from_bytes, table_exists};
use super::time::{firefox_time_to_dt, parse_iso_or_millis, unix_millis_to_dt};
use crate::browser::chromium::BrowserDownload;
use rusqlite::params;
use serde_json::Value;

pub fn parse_firefox_downloads(data: &[u8]) -> Result<Vec<BrowserDownload>, String> {
    if data.len() >= 16 && &data[..16] == b"SQLite format 3\0" {
        parse_from_sqlite(data)
    } else {
        parse_from_json(data)
    }
}

fn parse_from_sqlite(data: &[u8]) -> Result<Vec<BrowserDownload>, String> {
    let (conn, _tmp) = open_sqlite_from_bytes(data)?;
    if !table_exists(&conn, "moz_annos") || !table_exists(&conn, "moz_anno_attributes") {
        return Ok(Vec::new());
    }

    let mut stmt = conn
        .prepare(
            "SELECT a.place_id,
                    MAX(CASE WHEN attr.name = 'downloads/destinationFileURI'
                              OR attr.name = 'downloads/destinationFileName'
                             THEN a.content END) AS target_path,
                    MIN(a.dateAdded) AS start_time,
                    MAX(a.lastModified) AS end_time,
                    p.url AS source_url
             FROM moz_annos a
             JOIN moz_anno_attributes attr ON attr.id = a.anno_attribute_id
             LEFT JOIN moz_places p ON p.id = a.place_id
             WHERE attr.name IN ('downloads/destinationFileURI',
                                 'downloads/destinationFileName',
                                 'downloads/metaData')
             GROUP BY a.place_id
             ORDER BY start_time DESC",
        )
        .map_err(|e| format!("prepare moz_annos query: {}", e))?;
    let rows = stmt
        .query_map(params![], |row| {
            let start_time: Option<i64> = row.get(2).ok();
            let end_time: Option<i64> = row.get(3).ok();
            let source_url: Option<String> = row.get(4).ok();
            Ok(BrowserDownload {
                url: source_url.unwrap_or_default(),
                target_path: row.get(1).ok(),
                start_time: start_time.and_then(firefox_time_to_dt),
                end_time: end_time.and_then(firefox_time_to_dt),
                total_bytes: 0,
                browser: "Firefox".to_string(),
                profile: None,
            })
        })
        .map_err(|e| format!("query moz_annos: {}", e))?;

    let mut results = Vec::new();
    for row in rows {
        match row {
            Ok(download) => results.push(download),
            Err(err) => tracing::warn!("skipping firefox download row: {}", err),
        }
    }
    Ok(results)
}

fn parse_from_json(data: &[u8]) -> Result<Vec<BrowserDownload>, String> {
    let text =
        std::str::from_utf8(data).map_err(|e| format!("downloads.json is not UTF-8: {}", e))?;
    let root: Value =
        serde_json::from_str(text).map_err(|e| format!("downloads.json parse error: {}", e))?;
    let list = match root.get("list") {
        Some(Value::Array(entries)) => entries,
        _ if root.is_array() => root.as_array().expect("array checked above"),
        _ => return Ok(Vec::new()),
    };

    let mut results = Vec::new();
    for entry in list {
        let url = entry
            .get("source")
            .and_then(|source| source.get("url"))
            .and_then(Value::as_str)
            .unwrap_or("");
        let target_path = entry
            .get("target")
            .and_then(|target| target.get("path"))
            .and_then(Value::as_str);
        if url.is_empty() && target_path.is_none() {
            continue;
        }
        results.push(BrowserDownload {
            url: url.to_string(),
            target_path: target_path.map(str::to_string),
            start_time: parse_time(entry.get("startTime")),
            end_time: parse_time(entry.get("endTime")),
            total_bytes: entry
                .get("fileSize")
                .and_then(Value::as_i64)
                .unwrap_or(0)
                .max(0),
            browser: "Firefox".to_string(),
            profile: None,
        });
    }
    Ok(results)
}

fn parse_time(value: Option<&Value>) -> Option<chrono::DateTime<chrono::Utc>> {
    value.and_then(|value| {
        value
            .as_str()
            .and_then(parse_iso_or_millis)
            .or_else(|| value.as_i64().and_then(unix_millis_to_dt))
    })
}
