//! Firefox browser artifact parsers.
//!
//! Parsers work directly against raw file bytes: SQLite databases (places.sqlite,
//! cookies.sqlite), JSON files (downloads.json), and session restore files
//! (mozLz4-compressed or plain JSON).
//!
//! Each SQLite parser opens the database in-memory via a temporary file so the
//! caller never touches the host filesystem.
//!
//! All parsers gracefully return empty results when tables/columns are missing
//! rather than erroring.

use chrono::{DateTime, TimeZone, Utc};
use rusqlite::{params, Connection};
use serde_json::Value;
use std::io::Write;

use super::chromium::{
    BrowserCookie, BrowserDownload, BrowserPassword, BrowserSessionTab, BrowserVisit,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Convert Firefox PRTime (microseconds since 1970-01-01 UTC) to a chrono
/// `DateTime<Utc>`.  Used for `moz_places.last_visit_date`,
/// `moz_historyvisits.visit_date`, and `moz_cookies.creationTime`.
fn firefox_time_to_dt(microseconds: i64) -> Option<DateTime<Utc>> {
    if microseconds <= 0 {
        return None;
    }
    let secs = microseconds / 1_000_000;
    let nsecs = ((microseconds % 1_000_000) * 1000) as u32;
    Utc.timestamp_opt(secs, nsecs).single()
}

/// Convert Unix seconds to `DateTime<Utc>` (used for `moz_cookies.expiry`,
/// which Firefox stores as seconds since the Unix epoch).
fn unix_seconds_to_dt(seconds: i64) -> Option<DateTime<Utc>> {
    if seconds <= 0 {
        return None;
    }
    Utc.timestamp_opt(seconds, 0).single()
}

/// Convert Unix milliseconds to `DateTime<Utc>` (used for timestamps in
/// `downloads.json` and session store `lastAccessed` fields).
fn unix_millis_to_dt(millis: i64) -> Option<DateTime<Utc>> {
    if millis <= 0 {
        return None;
    }
    let secs = millis / 1000;
    let nsecs = ((millis % 1000) * 1_000_000) as u32;
    Utc.timestamp_opt(secs, nsecs).single()
}

/// Parse an ISO 8601 timestamp string (e.g. "2024-06-14T10:30:00.000Z") into
/// a `DateTime<Utc>`.  Falls back to parsing as integer milliseconds.
fn parse_iso_or_millis(s: &str) -> Option<DateTime<Utc>> {
    // Try ISO 8601 first
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Some(dt.with_timezone(&Utc));
    }
    // Try ISO 8601 without timezone (assume UTC)
    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.f") {
        return Some(Utc.from_utc_datetime(&dt));
    }
    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S") {
        return Some(Utc.from_utc_datetime(&dt));
    }
    // Fallback: parse as integer milliseconds
    if let Ok(millis) = s.parse::<i64>() {
        return unix_millis_to_dt(millis);
    }
    None
}

/// Open a SQLite connection from an in-memory byte slice by writing to a
/// temporary file.
fn open_sqlite_from_bytes(data: &[u8]) -> Result<(Connection, tempfile::NamedTempFile), String> {
    let mut tmp = tempfile::NamedTempFile::new().map_err(|e| format!("tempfile: {}", e))?;
    tmp.write_all(data)
        .map_err(|e| format!("write tempfile: {}", e))?;
    tmp.flush().map_err(|e| format!("flush tempfile: {}", e))?;
    let conn = Connection::open(tmp.path()).map_err(|e| format!("open sqlite: {}", e))?;
    Ok((conn, tmp))
}

/// Check whether a table exists in the open SQLite database.
fn table_exists(conn: &Connection, table: &str) -> bool {
    conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
        [table],
        |row| row.get::<_, i64>(0),
    )
    .map(|c| c > 0)
    .unwrap_or(false)
}

/// Heuristic: does a cookie value look like ciphertext rather than plaintext?
fn is_likely_encrypted(value: &str) -> bool {
    if value.len() < 8 {
        return false;
    }
    let bytes = value.as_bytes();
    let non_printable = bytes
        .iter()
        .filter(|&&b| !(0x20..=0x7e).contains(&b))
        .count();
    (non_printable as f64) > (bytes.len() as f64 * 0.3)
}

/// Decompress a mozLz4-compressed payload.
///
/// mozLz4 format:
///   - bytes 0..8: magic "mozLz40\0"
///   - bytes 8..12: uncompressed size (u32 little-endian)
///   - bytes 12..: LZ4 block-compressed data
fn decompress_mozlz4(data: &[u8]) -> Result<Vec<u8>, String> {
    const MAGIC: &[u8] = b"mozLz40\0";
    if data.len() < 12 {
        return Err("data too short for mozLz4 header".to_string());
    }
    if &data[..8] != MAGIC {
        return Err("not a mozLz4 stream".to_string());
    }
    let uncompressed_size = u32::from_le_bytes([data[8], data[9], data[10], data[11]]) as usize;
    // Safety bound: Firefox session files are typically < 100 MB uncompressed.
    if uncompressed_size > 512 * 1024 * 1024 {
        return Err(format!(
            "mozLz4 uncompressed size {} exceeds safety limit",
            uncompressed_size
        ));
    }
    let mut decompressed = vec![0u8; uncompressed_size];
    lz4_flex::block::decompress_into(&data[12..], &mut decompressed)
        .map_err(|e| format!("lz4 decompress: {}", e))?;
    Ok(decompressed)
}

// ---------------------------------------------------------------------------
// 1.  History parser (places.sqlite)
// ---------------------------------------------------------------------------

/// Parse Firefox browsing history from a `places.sqlite` database.
///
/// Queries `moz_places` joined with `moz_historyvisits` and returns one
/// `BrowserVisit` per visit row.  Falls back to `moz_places.last_visit_date`
/// when no matching visit row exists.
///
/// Firefox stores `visit_date` and `last_visit_date` as PRTime (microseconds
/// since 1970-01-01 UTC).
pub fn parse_firefox_history(data: &[u8]) -> Result<Vec<BrowserVisit>, String> {
    let (conn, _tmp) = open_sqlite_from_bytes(data)?;

    if !table_exists(&conn, "moz_places") {
        return Ok(Vec::new());
    }

    let mut stmt = conn
        .prepare(
            "SELECT p.url, p.title, p.visit_count, p.last_visit_date,
                    v.visit_date
             FROM moz_places p
             LEFT JOIN moz_historyvisits v ON v.place_id = p.id
             ORDER BY COALESCE(v.visit_date, p.last_visit_date) DESC",
        )
        .map_err(|e| format!("prepare firefox history query: {}", e))?;

    let rows = stmt
        .query_map(params![], |row| {
            let url: String = row.get(0)?;
            let title: Option<String> = row.get(1).ok();
            let visit_count: i64 = row.get(2).unwrap_or(0);
            let last_visit_raw: Option<i64> = row.get(3).ok();
            let visit_date_raw: Option<i64> = row.get(4).ok();

            // Prefer the explicit visit_date from moz_historyvisits;
            // fall back to moz_places.last_visit_date.
            let time_raw = visit_date_raw.or(last_visit_raw);

            Ok(BrowserVisit {
                url,
                title,
                visit_time: time_raw.and_then(firefox_time_to_dt),
                visit_count,
                browser: "Firefox".to_string(),
                profile: None,
            })
        })
        .map_err(|e| format!("query firefox history: {}", e))?;

    let mut results = Vec::new();
    for row in rows {
        match row {
            Ok(visit) => results.push(visit),
            Err(e) => {
                tracing::warn!("skipping firefox history row: {}", e);
            }
        }
    }

    Ok(results)
}

// ---------------------------------------------------------------------------
// 2.  Downloads parser (places.sqlite via moz_annos, or downloads.json)
// ---------------------------------------------------------------------------

/// Parse Firefox downloads.
///
/// Detects the input format automatically:
///   - SQLite (starts with `SQLite format 3\0`) -> reads `moz_annos` from
///     places.sqlite.
///   - Otherwise -> interprets the data as `downloads.json` (UTF-8 JSON).
pub fn parse_firefox_downloads(data: &[u8]) -> Result<Vec<BrowserDownload>, String> {
    if data.len() >= 16 && &data[..16] == b"SQLite format 3\0" {
        parse_firefox_downloads_from_sqlite(data)
    } else {
        parse_firefox_downloads_from_json(data)
    }
}

/// Extract download records from the `moz_annos` / `moz_anno_attributes`
/// tables inside places.sqlite.
///
/// Firefox stores download metadata as annotations keyed by attribute names
/// like `downloads/destinationFileURI`.  We pivot per `place_id` and join
/// with `moz_places` to recover the source URL.
fn parse_firefox_downloads_from_sqlite(data: &[u8]) -> Result<Vec<BrowserDownload>, String> {
    let (conn, _tmp) = open_sqlite_from_bytes(data)?;

    if !table_exists(&conn, "moz_annos") || !table_exists(&conn, "moz_anno_attributes") {
        return Ok(Vec::new());
    }

    // Aggregate download annotations per place_id.
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
            let _place_id: i64 = row.get(0).unwrap_or(0);
            let target_path: Option<String> = row.get(1).ok();
            let start_time_raw: Option<i64> = row.get(2).ok();
            let end_time_raw: Option<i64> = row.get(3).ok();
            let source_url: Option<String> = row.get(4).ok();

            Ok(BrowserDownload {
                url: source_url.unwrap_or_default(),
                target_path,
                start_time: start_time_raw.and_then(firefox_time_to_dt),
                end_time: end_time_raw.and_then(firefox_time_to_dt),
                total_bytes: 0,
                browser: "Firefox".to_string(),
                profile: None,
            })
        })
        .map_err(|e| format!("query moz_annos: {}", e))?;

    let mut results = Vec::new();
    for row in rows {
        match row {
            Ok(dl) => results.push(dl),
            Err(e) => {
                tracing::warn!("skipping firefox download row: {}", e);
            }
        }
    }

    Ok(results)
}

/// Parse Firefox downloads from a `downloads.json` file.
///
/// Expected structure:
/// ```json
/// { "list": [
///     { "target": { "path": "..." }, "source": { "url": "..." },
///       "startTime": "...", "endTime": "...", "fileSize": 12345 }
/// ] }
/// ```
///
/// Timestamps may be ISO 8601 strings or integer milliseconds since the
/// Unix epoch.
fn parse_firefox_downloads_from_json(data: &[u8]) -> Result<Vec<BrowserDownload>, String> {
    let text =
        std::str::from_utf8(data).map_err(|e| format!("downloads.json is not UTF-8: {}", e))?;

    let root: Value =
        serde_json::from_str(text).map_err(|e| format!("downloads.json parse error: {}", e))?;

    let list = match root.get("list") {
        Some(Value::Array(arr)) => arr,
        // If the top-level is already an array, use it directly.
        _ if root.is_array() => root.as_array().unwrap(),
        _ => return Ok(Vec::new()),
    };

    let mut results = Vec::new();
    for entry in list {
        let url = entry
            .get("source")
            .and_then(|s| s.get("url"))
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let target_path = entry
            .get("target")
            .and_then(|t| t.get("path"))
            .and_then(|v| v.as_str());

        let start_time = entry
            .get("startTime")
            .and_then(|v| v.as_str())
            .and_then(parse_iso_or_millis)
            .or_else(|| {
                entry
                    .get("startTime")
                    .and_then(|v| v.as_i64())
                    .and_then(unix_millis_to_dt)
            });

        let end_time = entry
            .get("endTime")
            .and_then(|v| v.as_str())
            .and_then(parse_iso_or_millis)
            .or_else(|| {
                entry
                    .get("endTime")
                    .and_then(|v| v.as_i64())
                    .and_then(unix_millis_to_dt)
            });

        let total_bytes = entry
            .get("fileSize")
            .and_then(|v| v.as_i64())
            .unwrap_or(0)
            .max(0);

        if url.is_empty() && target_path.is_none() {
            continue;
        }

        results.push(BrowserDownload {
            url: url.to_string(),
            target_path: target_path.map(|s| s.to_string()),
            start_time,
            end_time,
            total_bytes,
            browser: "Firefox".to_string(),
            profile: None,
        });
    }

    Ok(results)
}

// ---------------------------------------------------------------------------
// 3.  Cookies parser (cookies.sqlite)
// ---------------------------------------------------------------------------

/// Parse Firefox cookies from a `cookies.sqlite` database.
///
/// Reads the `moz_cookies` table.  The `value` column may contain plaintext
/// cookie values; `value_preview` will be `None` for values that appear to
/// be ciphertext (high-entropy binary data).
///
/// Firefox stores `expiry` as seconds since the Unix epoch; `creationTime`
/// and `lastAccessed` are PRTime (microseconds).
pub fn parse_firefox_cookies(data: &[u8]) -> Result<Vec<BrowserCookie>, String> {
    let (conn, _tmp) = open_sqlite_from_bytes(data)?;

    if !table_exists(&conn, "moz_cookies") {
        return Ok(Vec::new());
    }

    // Build the column list dynamically so we gracefully handle schema drift
    // (some older Firefox versions lack sameSite / isHttpOnly).
    let columns: Vec<String> = {
        let mut stmt = conn
            .prepare("PRAGMA table_info(moz_cookies)")
            .map_err(|e| format!("pragma: {}", e))?;
        let rows = stmt
            .query_map([], |row| row.get::<_, String>(1))
            .map_err(|e| format!("pragma rows: {}", e))?;
        let mut cols = Vec::new();
        for r in rows {
            cols.push(r.map_err(|e| format!("pragma row: {}", e))?);
        }
        cols
    };

    let has_same_site = columns.iter().any(|c| c == "sameSite");
    let has_http_only = columns.iter().any(|c| c == "isHttpOnly");
    let has_secure = columns.iter().any(|c| c == "isSecure");

    // Build the SELECT list.
    let mut select_cols = vec![
        "baseDomain".to_string(),
        "name".to_string(),
        "value".to_string(),
        "expiry".to_string(),
    ];
    if has_secure {
        select_cols.push("isSecure".to_string());
    } else {
        select_cols.push("0 AS isSecure".to_string());
    }
    if has_http_only {
        select_cols.push("isHttpOnly".to_string());
    } else {
        select_cols.push("0 AS isHttpOnly".to_string());
    }
    if has_same_site {
        select_cols.push("sameSite".to_string());
    } else {
        select_cols.push("NULL AS sameSite".to_string());
    }

    let sql = format!(
        "SELECT {} FROM moz_cookies ORDER BY baseDomain",
        select_cols.join(", ")
    );

    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| format!("prepare cookies query: {}", e))?;

    let rows = stmt
        .query_map(params![], |row| {
            let domain: String = row.get(0)?;
            let name: String = row.get(1)?;
            let raw_value: Option<String> = row.get(2).ok();

            let value_preview = raw_value.as_ref().and_then(|v| {
                if v.is_empty() {
                    None
                } else if is_likely_encrypted(v) {
                    Some(format!("[encrypted {} bytes]", v.len()))
                } else {
                    let preview: String = v.chars().take(128).collect();
                    Some(preview)
                }
            });

            let expiry_raw: i64 = row.get(3).unwrap_or(0);
            let secure: bool = row.get::<_, i64>(4).map(|v| v != 0).unwrap_or(false);
            let http_only: bool = row.get::<_, i64>(5).map(|v| v != 0).unwrap_or(false);
            let same_site: Option<i64> = row.get(6).ok();

            Ok(BrowserCookie {
                domain,
                name,
                value_preview,
                expiry: unix_seconds_to_dt(expiry_raw),
                secure,
                http_only,
                same_site,
            })
        })
        .map_err(|e| format!("query cookies: {}", e))?;

    let mut results = Vec::new();
    for row in rows {
        match row {
            Ok(c) => results.push(c),
            Err(e) => {
                tracing::warn!("skipping firefox cookie row: {}", e);
            }
        }
    }

    Ok(results)
}

// ---------------------------------------------------------------------------
// 5.  Passwords parser (logins.json)
// ---------------------------------------------------------------------------

/// Parse Firefox saved passwords from `logins.json`.
///
/// The `encryptedUsername`/`encryptedPassword` fields are encrypted by Firefox's
/// internal key store; this parser extracts only metadata and never decrypts.
pub fn parse_firefox_passwords(data: &[u8]) -> Result<Vec<BrowserPassword>, String> {
    let text =
        std::str::from_utf8(data).map_err(|e| format!("logins.json is not valid UTF-8: {}", e))?;

    let root: Value =
        serde_json::from_str(text).map_err(|e| format!("logins.json parse error: {}", e))?;

    let logins = match root.get("logins") {
        Some(Value::Array(arr)) => arr,
        _ => return Ok(Vec::new()),
    };

    let mut results = Vec::new();
    for entry in logins {
        let hostname = entry
            .get("hostname")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let username = entry
            .get("encryptedUsername")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let password = entry
            .get("encryptedPassword")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let created_at = entry
            .get("timeCreated")
            .and_then(|v| v.as_i64())
            .and_then(unix_millis_to_dt);
        let times_used = entry
            .get("timesUsed")
            .and_then(|v| v.as_i64())
            .unwrap_or(0)
            .max(0);

        let password_preview = if password.is_empty() {
            None
        } else {
            Some(format!("[encrypted {} bytes]", password.len()))
        };

        results.push(BrowserPassword {
            url: hostname,
            username,
            password_preview,
            created_at,
            times_used,
            browser: "Firefox".to_string(),
            profile: None,
        });
    }

    Ok(results)
}

// ---------------------------------------------------------------------------
// 4.  Session parser (sessionstore-backups/recovery.jsonlz4 et al.)
// ---------------------------------------------------------------------------

/// Parse a Firefox session restore file.
///
/// Supports two formats:
///   - **mozLz4** (`.jsonlz4`): magic `mozLz40\0`, 4-byte LE uncompressed
///     size, then LZ4 block data.  Decompresses, then parses as JSON.
///   - **Plain JSON** (`.js`): parsed directly.
///
/// The session store JSON has a `windows` array; each window has a `tabs`
/// array; each tab has an `entries` array of `{url, title}` objects.  The
/// active entry is indicated by `index`.
pub fn parse_firefox_session(data: &[u8]) -> Result<Vec<BrowserSessionTab>, String> {
    let json_bytes: Vec<u8> = if data.len() >= 8 && &data[..8] == b"mozLz40\0" {
        decompress_mozlz4(data)?
    } else {
        data.to_vec()
    };

    let text = std::str::from_utf8(&json_bytes)
        .map_err(|e| format!("session file is not valid UTF-8: {}", e))?;

    let root: Value =
        serde_json::from_str(text).map_err(|e| format!("session JSON parse error: {}", e))?;

    let mut results = Vec::new();

    let windows = match root.get("windows") {
        Some(Value::Array(wins)) => wins,
        _ => {
            // Not a standard session store; return empty.
            return Ok(results);
        }
    };

    for (wi, window) in windows.iter().enumerate() {
        let window_index = window
            .get("index")
            .and_then(|v| v.as_i64())
            .unwrap_or(wi as i64) as i32;

        let tabs = match window.get("tabs") {
            Some(Value::Array(tabs)) => tabs,
            _ => continue,
        };

        for (ti, tab) in tabs.iter().enumerate() {
            let tab_index = ti as i32;

            // Determine which entry is the active (selected) one.
            // Firefox uses a 1-based `index` on the tab object.
            let active_index = tab
                .get("index")
                .and_then(|v| v.as_i64())
                .map(|i| (i - 1).max(0) as usize)
                .unwrap_or(0);

            let last_active = tab
                .get("lastAccessed")
                .and_then(|v| v.as_i64())
                .and_then(unix_millis_to_dt);

            let entries = match tab.get("entries") {
                Some(Value::Array(entries)) => entries,
                // Some versions inline url/title directly on the tab object.
                _ => {
                    if let Some(tab_result) =
                        parse_session_tab_entry(tab, window_index, tab_index, last_active)
                    {
                        results.push(tab_result);
                    }
                    continue;
                }
            };

            // Emit the active entry (the one the user was viewing).
            if let Some(entry) = entries.get(active_index) {
                if let Some(tab_result) =
                    parse_session_tab_entry(entry, window_index, tab_index, last_active)
                {
                    results.push(tab_result);
                }
            } else {
                // Fallback: emit the first entry with a URL.
                for entry in entries.iter() {
                    if let Some(tab_result) =
                        parse_session_tab_entry(entry, window_index, tab_index, last_active)
                    {
                        results.push(tab_result);
                        break;
                    }
                }
            }
        }
    }

    Ok(results)
}

/// Extract a single `BrowserSessionTab` from a JSON entry (or tab) object.
fn parse_session_tab_entry(
    entry: &Value,
    window_index: i32,
    tab_index: i32,
    last_active: Option<DateTime<Utc>>,
) -> Option<BrowserSessionTab> {
    let url = entry
        .get("url")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())?;

    let title = entry
        .get("title")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());

    Some(BrowserSessionTab {
        url: url.to_string(),
        title,
        window_index,
        tab_index,
        last_active,
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Datelike;
    use std::io::Read;

    // ------------------------------------------------------------------
    // Test helpers
    // ------------------------------------------------------------------

    /// Build an in-memory `places.sqlite` with `moz_places` and
    /// `moz_historyvisits` tables, then return the raw bytes.
    fn make_firefox_places_db() -> Vec<u8> {
        let mut tmp = tempfile::NamedTempFile::new().expect("tempfile");
        {
            let conn = Connection::open(tmp.path()).expect("open db");
            conn.execute_batch(
                "CREATE TABLE moz_places (
                    id INTEGER PRIMARY KEY,
                    url TEXT,
                    title TEXT,
                    visit_count INTEGER DEFAULT 0,
                    last_visit_date INTEGER
                );
                CREATE TABLE moz_historyvisits (
                    id INTEGER PRIMARY KEY,
                    place_id INTEGER,
                    visit_date INTEGER
                );
                INSERT INTO moz_places VALUES (1, 'https://www.mozilla.org', 'Mozilla', 10, 1718352000000000);
                INSERT INTO moz_historyvisits VALUES (1, 1, 1718352000000000);
                INSERT INTO moz_historyvisits VALUES (2, 1, 1718438400000000);",
            )
            .expect("batch");
        }
        let mut buf = Vec::new();
        tmp.read_to_end(&mut buf).expect("read tmp");
        buf
    }

    fn make_empty_firefox_places_db() -> Vec<u8> {
        let mut tmp = tempfile::NamedTempFile::new().expect("tempfile");
        {
            let conn = Connection::open(tmp.path()).expect("open db");
            conn.execute_batch(
                "CREATE TABLE moz_places (
                    id INTEGER PRIMARY KEY,
                    url TEXT,
                    title TEXT,
                    visit_count INTEGER,
                    last_visit_date INTEGER
                );
                CREATE TABLE moz_historyvisits (
                    id INTEGER PRIMARY KEY,
                    place_id INTEGER,
                    visit_date INTEGER
                );",
            )
            .expect("batch");
        }
        let mut buf = Vec::new();
        tmp.read_to_end(&mut buf).expect("read tmp");
        buf
    }

    fn make_firefox_cookies_db() -> Vec<u8> {
        let mut tmp = tempfile::NamedTempFile::new().expect("tempfile");
        {
            let conn = Connection::open(tmp.path()).expect("open db");
            conn.execute_batch(
                "CREATE TABLE moz_cookies (
                    id INTEGER PRIMARY KEY,
                    baseDomain TEXT,
                    name TEXT,
                    value TEXT,
                    host TEXT,
                    path TEXT,
                    expiry INTEGER,
                    lastAccessed INTEGER,
                    creationTime INTEGER,
                    isSecure INTEGER,
                    isHttpOnly INTEGER,
                    sameSite INTEGER
                );
                INSERT INTO moz_cookies VALUES (
                    1, 'mozilla.org', 'session', 'abc123', 'www.mozilla.org', '/',
                    1718352000, 1718352000000000, 1718352000000000,
                    1, 1, 2
                );
                INSERT INTO moz_cookies VALUES (
                    2, 'example.com', 'tracker', 'xyz789', '.example.com', '/',
                    0, 1718352000000000, 1718352000000000,
                    0, 0, 0
                );",
            )
            .expect("batch");
        }
        let mut buf = Vec::new();
        tmp.read_to_end(&mut buf).expect("read tmp");
        buf
    }

    // ------------------------------------------------------------------
    // History
    // ------------------------------------------------------------------

    #[test]
    fn parse_firefox_history_basic() {
        let db = make_firefox_places_db();
        let visits = parse_firefox_history(&db).expect("parse history");
        // Two visit rows for the same place
        assert_eq!(visits.len(), 2);
        assert_eq!(visits[0].url, "https://www.mozilla.org");
        assert_eq!(visits[0].title.as_deref(), Some("Mozilla"));
        assert_eq!(visits[0].visit_count, 10);
        assert!(visits[0].visit_time.is_some());
        assert_eq!(visits[0].browser, "Firefox");
        assert!(visits[0].profile.is_none());
    }

    #[test]
    fn parse_firefox_history_empty_db() {
        let db = make_empty_firefox_places_db();
        let visits = parse_firefox_history(&db).expect("parse");
        assert!(visits.is_empty());
    }

    #[test]
    fn parse_firefox_history_not_a_db() {
        // Non-SQLite data gracefully returns empty rather than erroring
        // (table_exists silently fails and returns false).
        let visits = parse_firefox_history(b"not sqlite").expect("parse");
        assert!(visits.is_empty());
    }

    #[test]
    fn parse_firefox_history_no_moz_places() {
        let mut tmp = tempfile::NamedTempFile::new().expect("tempfile");
        {
            let conn = Connection::open(tmp.path()).expect("open db");
            conn.execute_batch("CREATE TABLE unrelated (id INTEGER);")
                .expect("batch");
        }
        let mut buf = Vec::new();
        tmp.read_to_end(&mut buf).expect("read tmp");
        let visits = parse_firefox_history(&buf).expect("parse");
        assert!(visits.is_empty());
    }

    // ------------------------------------------------------------------
    // Timestamp conversion
    // ------------------------------------------------------------------

    #[test]
    fn firefox_time_conversion() {
        // 1718352000000000 = 2024-06-14T08:00:00Z
        let dt = firefox_time_to_dt(1718352000000000);
        assert!(dt.is_some());
        let dt = dt.unwrap();
        assert_eq!(dt.year_ce(), (true, 2024));
        assert_eq!(dt.month(), 6);
        assert_eq!(dt.day(), 14);
    }

    #[test]
    fn firefox_time_zero_is_none() {
        assert!(firefox_time_to_dt(0).is_none());
        assert!(firefox_time_to_dt(-1).is_none());
    }

    #[test]
    fn unix_seconds_conversion() {
        // 1718352000 = 2024-06-14T08:00:00Z
        let dt = unix_seconds_to_dt(1718352000);
        assert!(dt.is_some());
        let dt = dt.unwrap();
        assert_eq!(dt.year_ce(), (true, 2024));
        assert_eq!(dt.month(), 6);
    }

    #[test]
    fn unix_millis_conversion() {
        // 1718352000000 = 2024-06-14T08:00:00Z
        let dt = unix_millis_to_dt(1718352000000);
        assert!(dt.is_some());
        let dt = dt.unwrap();
        assert_eq!(dt.year_ce(), (true, 2024));
        assert_eq!(dt.month(), 6);
    }

    #[test]
    fn parse_iso_timestamp() {
        let dt = parse_iso_or_millis("2024-06-14T08:00:00.000Z");
        assert!(dt.is_some());
        let dt = dt.unwrap();
        assert_eq!(dt.year_ce(), (true, 2024));
    }

    #[test]
    fn parse_iso_no_timezone() {
        let dt = parse_iso_or_millis("2024-06-14T08:00:00");
        assert!(dt.is_some());
        let dt = dt.unwrap();
        assert_eq!(dt.year_ce(), (true, 2024));
    }

    #[test]
    fn parse_millis_string() {
        let dt = parse_iso_or_millis("1718352000000");
        assert!(dt.is_some());
        let dt = dt.unwrap();
        assert_eq!(dt.year_ce(), (true, 2024));
    }

    // ------------------------------------------------------------------
    // Downloads via downloads.json
    // ------------------------------------------------------------------

    #[test]
    fn parse_firefox_downloads_json_basic() {
        let json = r#"{
            "list": [
                {
                    "target": {"path": "C:\\Users\\test\\Downloads\\firefox.exe"},
                    "source": {"url": "https://www.mozilla.org/firefox.exe"},
                    "startTime": "2024-06-14T08:00:00.000Z",
                    "endTime": "2024-06-14T08:01:00.000Z",
                    "fileSize": 50000000
                }
            ]
        }"#;
        let downloads = parse_firefox_downloads(json.as_bytes()).expect("parse");
        assert_eq!(downloads.len(), 1);
        assert_eq!(downloads[0].url, "https://www.mozilla.org/firefox.exe");
        assert_eq!(
            downloads[0].target_path.as_deref(),
            Some("C:\\Users\\test\\Downloads\\firefox.exe")
        );
        assert_eq!(downloads[0].total_bytes, 50000000);
        assert!(downloads[0].start_time.is_some());
        assert!(downloads[0].end_time.is_some());
        assert_eq!(downloads[0].browser, "Firefox");
    }

    #[test]
    fn parse_firefox_downloads_json_top_level_array() {
        let json = r#"[
            {"source": {"url": "https://a.com/a.zip"}, "target": {"path": "/tmp/a.zip"}},
            {"source": {"url": "https://b.com/b.zip"}, "target": {"path": "/tmp/b.zip"}}
        ]"#;
        let downloads = parse_firefox_downloads(json.as_bytes()).expect("parse");
        assert_eq!(downloads.len(), 2);
        assert_eq!(downloads[0].url, "https://a.com/a.zip");
        assert_eq!(downloads[1].url, "https://b.com/b.zip");
    }

    #[test]
    fn parse_firefox_downloads_json_empty() {
        let downloads = parse_firefox_downloads(b"{}").expect("parse");
        assert!(downloads.is_empty());
    }

    #[test]
    fn parse_firefox_downloads_json_millis_timestamps() {
        let json = r#"{
            "list": [
                {
                    "target": {"path": "/tmp/file"},
                    "source": {"url": "https://example.com/file"},
                    "startTime": 1718352000000,
                    "endTime": 1718352060000
                }
            ]
        }"#;
        let downloads = parse_firefox_downloads(json.as_bytes()).expect("parse");
        assert_eq!(downloads.len(), 1);
        assert!(downloads[0].start_time.is_some());
        assert!(downloads[0].end_time.is_some());
    }

    #[test]
    fn parse_firefox_downloads_skips_empty_entries() {
        let json = r#"{
            "list": [
                {"source": {"url": ""}, "target": {}},
                {"source": {"url": "https://valid.com/file"}, "target": {"path": "/tmp/valid"}}
            ]
        }"#;
        let downloads = parse_firefox_downloads(json.as_bytes()).expect("parse");
        // The first entry has no url AND no target_path -> skipped.
        assert_eq!(downloads.len(), 1);
        assert_eq!(downloads[0].url, "https://valid.com/file");
    }

    // ------------------------------------------------------------------
    // Cookies
    // ------------------------------------------------------------------

    #[test]
    fn parse_firefox_cookies_basic() {
        let db = make_firefox_cookies_db();
        let cookies = parse_firefox_cookies(&db).expect("parse cookies");
        assert_eq!(cookies.len(), 2);

        // ORDER BY baseDomain: "example.com" < "mozilla.org"
        // First row: example.com / tracker (no expiry, no secure)
        assert_eq!(cookies[0].domain, "example.com");
        assert_eq!(cookies[0].name, "tracker");
        assert!(cookies[0].expiry.is_none()); // expiry = 0 -> None
        assert!(!cookies[0].secure);
        assert!(!cookies[0].http_only);
        assert_eq!(cookies[0].same_site, Some(0)); // none

        // Second row: mozilla.org / session (with expiry, secure, httpOnly, strict)
        assert_eq!(cookies[1].domain, "mozilla.org");
        assert_eq!(cookies[1].name, "session");
        assert_eq!(cookies[1].value_preview.as_deref(), Some("abc123"));
        assert!(cookies[1].expiry.is_some());
        assert!(cookies[1].secure);
        assert!(cookies[1].http_only);
        assert_eq!(cookies[1].same_site, Some(2)); // strict
    }

    #[test]
    fn parse_firefox_cookies_no_table() {
        let mut tmp = tempfile::NamedTempFile::new().expect("tempfile");
        {
            let conn = Connection::open(tmp.path()).expect("open db");
            conn.execute_batch("CREATE TABLE unrelated (id INTEGER);")
                .expect("batch");
        }
        let mut buf = Vec::new();
        tmp.read_to_end(&mut buf).expect("read tmp");
        let cookies = parse_firefox_cookies(&buf).expect("parse");
        assert!(cookies.is_empty());
    }

    #[test]
    fn parse_firefox_cookies_encrypted_value() {
        let mut tmp = tempfile::NamedTempFile::new().expect("tempfile");
        {
            let conn = Connection::open(tmp.path()).expect("open db");
            conn.execute_batch(
                "CREATE TABLE moz_cookies (
                    id INTEGER PRIMARY KEY, baseDomain TEXT, name TEXT, value TEXT,
                    host TEXT, path TEXT, expiry INTEGER, lastAccessed INTEGER,
                    creationTime INTEGER, isSecure INTEGER, isHttpOnly INTEGER,
                    sameSite INTEGER
                );",
            )
            .expect("batch");

            // Build a string with mostly non-printable control characters to
            // trigger the encryption heuristic (> 30 % non-printable).
            let mut enc_value = String::from("aaaa");
            for _ in 0..12 {
                enc_value.push('\x01');
            }
            conn.execute(
                "INSERT INTO moz_cookies VALUES (1, 'example.com', 'enc', ?1, \
                 'example.com', '/', 1718352000, 0, 0, 0, 0, 0)",
                rusqlite::params![enc_value],
            )
            .expect("insert");
        }
        let mut buf = Vec::new();
        tmp.read_to_end(&mut buf).expect("read tmp");
        let cookies = parse_firefox_cookies(&buf).expect("parse");
        assert_eq!(cookies.len(), 1);
        // The value is mostly non-printable -> should be marked as encrypted.
        assert!(cookies[0]
            .value_preview
            .as_deref()
            .unwrap()
            .starts_with("[encrypted"));
    }

    // ------------------------------------------------------------------
    // Session (mozLz4)
    // ------------------------------------------------------------------

    /// Build a valid mozLz4 payload from a JSON string.
    fn make_mozlz4(json: &str) -> Vec<u8> {
        let raw = json.as_bytes();
        let compressed = lz4_flex::block::compress(raw);
        let mut out = Vec::with_capacity(12 + compressed.len());
        out.extend_from_slice(b"mozLz40\0");
        out.extend_from_slice(&(raw.len() as u32).to_le_bytes());
        out.extend_from_slice(&compressed);
        out
    }

    #[test]
    fn parse_firefox_session_mozlz4_basic() {
        let json = r#"{
            "windows": [
                {
                    "tabs": [
                        {
                            "entries": [
                                {"url": "https://www.mozilla.org", "title": "Mozilla"},
                                {"url": "https://addons.mozilla.org", "title": "Add-ons"}
                            ],
                            "index": 2,
                            "lastAccessed": 1718352000000
                        }
                    ]
                }
            ]
        }"#;
        let compressed = make_mozlz4(json);
        let tabs = parse_firefox_session(&compressed).expect("parse session");
        assert_eq!(tabs.len(), 1);
        assert_eq!(tabs[0].url, "https://addons.mozilla.org"); // index 2 -> second entry
        assert_eq!(tabs[0].title.as_deref(), Some("Add-ons"));
        assert_eq!(tabs[0].window_index, 0);
        assert_eq!(tabs[0].tab_index, 0);
        assert!(tabs[0].last_active.is_some());
    }

    #[test]
    fn parse_firefox_session_plain_json() {
        let json = r#"{
            "windows": [
                {
                    "tabs": [
                        {
                            "entries": [
                                {"url": "https://www.example.com", "title": "Example"}
                            ],
                            "index": 1
                        }
                    ]
                }
            ]
        }"#;
        let tabs = parse_firefox_session(json.as_bytes()).expect("parse plain json");
        assert_eq!(tabs.len(), 1);
        assert_eq!(tabs[0].url, "https://www.example.com");
    }

    #[test]
    fn parse_firefox_session_multiple_windows_and_tabs() {
        let json = r#"{
            "windows": [
                {
                    "index": 0,
                    "tabs": [
                        {
                            "entries": [
                                {"url": "https://w0t0.com", "title": "W0T0"}
                            ],
                            "index": 1,
                            "lastAccessed": 1000
                        },
                        {
                            "entries": [
                                {"url": "https://w0t1.com", "title": "W0T1"}
                            ],
                            "index": 1,
                            "lastAccessed": 2000
                        }
                    ]
                },
                {
                    "index": 1,
                    "tabs": [
                        {
                            "entries": [
                                {"url": "https://w1t0.com", "title": "W1T0"},
                                {"url": "https://w1t0-page2.com", "title": "W1T0 P2"}
                            ],
                            "index": 2,
                            "lastAccessed": 3000
                        }
                    ]
                }
            ]
        }"#;
        let tabs = parse_firefox_session(json.as_bytes()).expect("parse");
        assert_eq!(tabs.len(), 3);
        assert_eq!(tabs[0].url, "https://w0t0.com");
        assert_eq!(tabs[0].window_index, 0);
        assert_eq!(tabs[0].tab_index, 0);
        assert_eq!(tabs[1].url, "https://w0t1.com");
        assert_eq!(tabs[1].window_index, 0);
        assert_eq!(tabs[1].tab_index, 1);
        assert_eq!(tabs[2].url, "https://w1t0-page2.com"); // index 2 -> second entry
        assert_eq!(tabs[2].window_index, 1);
        assert_eq!(tabs[2].tab_index, 0);
    }

    #[test]
    fn parse_firefox_session_inline_tabs() {
        // Some older session formats have url/title directly on the tab.
        let json = r#"{
            "windows": [
                {
                    "tabs": [
                        {"url": "https://direct.com", "title": "Direct"}
                    ]
                }
            ]
        }"#;
        let tabs = parse_firefox_session(json.as_bytes()).expect("parse");
        assert_eq!(tabs.len(), 1);
        assert_eq!(tabs[0].url, "https://direct.com");
    }

    #[test]
    fn parse_firefox_session_empty_json() {
        let tabs = parse_firefox_session(b"{}").expect("parse");
        assert!(tabs.is_empty());
    }

    #[test]
    fn parse_firefox_session_invalid_utf8() {
        let result = parse_firefox_session(&[0xff, 0xfe, 0x00, 0x00]);
        assert!(result.is_err());
    }

    #[test]
    fn parse_firefox_session_skips_entries_without_url() {
        let json = r#"{
            "windows": [
                {
                    "tabs": [
                        {
                            "entries": [
                                {"title": "no url here"},
                                {"url": "https://valid.com", "title": "Valid"}
                            ],
                            "index": 2
                        }
                    ]
                }
            ]
        }"#;
        let tabs = parse_firefox_session(json.as_bytes()).expect("parse");
        assert_eq!(tabs.len(), 1);
        assert_eq!(tabs[0].url, "https://valid.com");
    }

    #[test]
    fn mozlz4_roundtrip() {
        let original = "hello firefox session data";
        let compressed = make_mozlz4(original);
        let decompressed = decompress_mozlz4(&compressed).expect("decompress");
        assert_eq!(decompressed, original.as_bytes());
    }

    #[test]
    fn mozlz4_rejects_bad_magic() {
        let result = decompress_mozlz4(b"not mozLz4 data!!!");
        assert!(result.is_err());
    }

    #[test]
    fn mozlz4_rejects_short_data() {
        let result = decompress_mozlz4(b"short");
        assert!(result.is_err());
    }

    // ------------------------------------------------------------------
    // Downloads via SQLite (moz_annos)
    // ------------------------------------------------------------------

    #[test]
    fn parse_firefox_downloads_from_sqlite_basic() {
        let mut tmp = tempfile::NamedTempFile::new().expect("tempfile");
        {
            let conn = Connection::open(tmp.path()).expect("open db");
            conn.execute_batch(
                "CREATE TABLE moz_places (id INTEGER PRIMARY KEY, url TEXT, title TEXT);
                 CREATE TABLE moz_anno_attributes (id INTEGER PRIMARY KEY, name TEXT);
                 CREATE TABLE moz_annos (
                     id INTEGER PRIMARY KEY,
                     place_id INTEGER,
                     anno_attribute_id INTEGER,
                     content TEXT,
                     flags INTEGER DEFAULT 0,
                     expiration INTEGER DEFAULT 0,
                     type INTEGER DEFAULT 3,
                     dateAdded INTEGER,
                     lastModified INTEGER
                 );
                 INSERT INTO moz_places VALUES (1, 'https://example.com/file.zip', 'Download page');
                 INSERT INTO moz_anno_attributes VALUES (1, 'downloads/destinationFileURI');
                 INSERT INTO moz_anno_attributes VALUES (2, 'downloads/metaData');
                 INSERT INTO moz_annos VALUES
                     (1, 1, 1, '/tmp/file.zip', 0, 0, 3, 1718352000000000, 1718352000000000),
                     (2, 1, 2, '{}', 0, 0, 3, 1718352000000000, 1718352060000000);",
            )
            .expect("batch");
        }
        let mut buf = Vec::new();
        tmp.read_to_end(&mut buf).expect("read tmp");
        let downloads = parse_firefox_downloads(&buf).expect("parse downloads");
        assert_eq!(downloads.len(), 1);
        assert_eq!(downloads[0].url, "https://example.com/file.zip");
        assert_eq!(downloads[0].target_path.as_deref(), Some("/tmp/file.zip"));
        assert!(downloads[0].start_time.is_some());
        assert_eq!(downloads[0].browser, "Firefox");
    }

    #[test]
    fn parse_firefox_downloads_from_sqlite_no_tables() {
        let mut tmp = tempfile::NamedTempFile::new().expect("tempfile");
        {
            let conn = Connection::open(tmp.path()).expect("open db");
            conn.execute_batch("CREATE TABLE unrelated (id INTEGER);")
                .expect("batch");
        }
        let mut buf = Vec::new();
        tmp.read_to_end(&mut buf).expect("read tmp");
        let downloads = parse_firefox_downloads(&buf).expect("parse");
        assert!(downloads.is_empty());
    }

    // ------------------------------------------------------------------
    // Encryption heuristic
    // ------------------------------------------------------------------

    #[test]
    fn is_likely_encrypted_plain_text() {
        assert!(!is_likely_encrypted("session=abc123"));
        assert!(!is_likely_encrypted(""));
    }

    #[test]
    fn is_likely_encrypted_binary_blob() {
        let mut raw = vec![b'a'; 20];
        raw.extend(vec![0x00u8; 20]);
        let mixed = String::from_utf8_lossy(&raw).into_owned();
        assert!(is_likely_encrypted(&mixed));
    }

    #[test]
    fn is_likely_encrypted_short_value() {
        assert!(!is_likely_encrypted("\x00\x01\x02"));
    }
}
