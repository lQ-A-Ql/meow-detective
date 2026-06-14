//! Chromium-based browser artifact parsers (Chrome, Edge, Brave, Opera, etc.).
//!
//! Parsers work directly against the raw SQLite database bytes (History, Cookies, Downloads)
//! and the Session Storage JSON files (Last Session / Last Tabs).  Each parser opens the
//! database in-memory via a temporary file so the caller never touches the host filesystem.
//!
//! All parsers handle schema variations gracefully: missing columns produce `None` values
//! rather than errors.

use chrono::{DateTime, TimeZone, Utc};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::io::Write;

// ---------------------------------------------------------------------------
// Public data structures
// ---------------------------------------------------------------------------

/// A single browser history visit (navigation record).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserVisit {
    pub url: String,
    pub title: Option<String>,
    pub visit_time: Option<DateTime<Utc>>,
    pub visit_count: i64,
    pub browser: String,
    pub profile: Option<String>,
}

/// A browser download record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserDownload {
    pub url: String,
    pub target_path: Option<String>,
    pub start_time: Option<DateTime<Utc>>,
    pub end_time: Option<DateTime<Utc>>,
    pub total_bytes: i64,
    pub browser: String,
    pub profile: Option<String>,
}

/// A browser cookie record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserCookie {
    pub domain: String,
    pub name: String,
    /// Truncated preview of the cookie value (first 128 bytes).
    /// Will be `None` for encrypted values that appear to be ciphertext.
    pub value_preview: Option<String>,
    pub expiry: Option<DateTime<Utc>>,
    pub secure: bool,
    pub http_only: bool,
    /// Raw `same_site` column value: -1 = unspecified, 0 = none, 1 = lax, 2 = strict.
    pub same_site: Option<i64>,
}

/// A single tab entry from a restored browser session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserSessionTab {
    pub url: String,
    pub title: Option<String>,
    pub window_index: i32,
    pub tab_index: i32,
    pub last_active: Option<DateTime<Utc>>,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Convert WebKit/Chrome timestamp (microseconds since 1601-01-01 UTC) to
/// a chrono `DateTime<Utc>`.
fn webkit_time_to_dt(microseconds: i64) -> Option<DateTime<Utc>> {
    if microseconds <= 0 {
        return None;
    }
    // 11_644_473_600 = seconds between 1601-01-01 (WebKit epoch) and 1970-01-01 (Unix epoch)
    let secs = microseconds / 1_000_000 - 11_644_473_600;
    let nsecs = ((microseconds % 1_000_000) * 1000) as u32;
    Utc.timestamp_opt(secs, nsecs).single()
}

/// Open a SQLite connection from an in-memory byte slice by writing to a
/// temporary file.  `rusqlite` (bundled) opens the db read-write; we use a
/// temp file that is automatically cleaned up when the connection is dropped.
fn open_sqlite_from_bytes(data: &[u8]) -> Result<(Connection, tempfile::NamedTempFile), String> {
    let mut tmp = tempfile::NamedTempFile::new().map_err(|e| format!("tempfile: {}", e))?;
    tmp.write_all(data)
        .map_err(|e| format!("write tempfile: {}", e))?;
    tmp.flush().map_err(|e| format!("flush tempfile: {}", e))?;
    let conn = Connection::open(tmp.path()).map_err(|e| format!("open sqlite: {}", e))?;
    Ok((conn, tmp))
}

/// Try to read a column value from a rusqlite `Row`, returning `None` when the
/// column is missing rather than panicking / erroring.
fn row_get_opt<T: rusqlite::types::FromSql>(row: &rusqlite::Row, col: &str) -> Option<T> {
    row.get(col).ok()
}

/// Heuristic to decide whether a raw cookie value looks like ciphertext
/// (high-entropy binary blob) or human-readable text.
fn is_likely_encrypted(value: &str) -> bool {
    if value.len() < 8 {
        return false;
    }
    let bytes = value.as_bytes();
    // If more than 30% of the bytes are non-printable, treat as encrypted.
    let non_printable = bytes
        .iter()
        .filter(|&&b| !(0x20..=0x7e).contains(&b))
        .count();
    (non_printable as f64) > (bytes.len() as f64 * 0.3)
}

// ---------------------------------------------------------------------------
// History parser
// ---------------------------------------------------------------------------

/// Parse a Chromium `History` SQLite database.
///
/// Queries the `urls` and `visits` tables and returns one `BrowserVisit` for
/// each visit row.
pub fn parse_chrome_history(
    data: &[u8],
    browser: &str,
    profile: Option<&str>,
) -> Result<Vec<BrowserVisit>, String> {
    let (conn, _tmp) = open_sqlite_from_bytes(data)?;

    let mut stmt = conn
        .prepare(
            "SELECT u.url, u.title, v.visit_time, u.visit_count
             FROM urls u
             JOIN visits v ON u.id = v.url
             ORDER BY v.visit_time DESC",
        )
        .map_err(|e| format!("prepare history query: {}", e))?;

    let rows = stmt
        .query_map(params![], |row| {
            let url: String = row.get(0)?;
            let title: Option<String> = row.get(1).ok();
            let visit_time_raw: i64 = row.get(2).unwrap_or(0);
            let visit_count: i64 = row.get(3).unwrap_or(1);
            Ok(BrowserVisit {
                url,
                title,
                visit_time: webkit_time_to_dt(visit_time_raw),
                visit_count,
                browser: browser.to_string(),
                profile: profile.map(|s| s.to_string()),
            })
        })
        .map_err(|e| format!("query history: {}", e))?;

    let mut results = Vec::new();
    for row in rows {
        match row {
            Ok(visit) => results.push(visit),
            Err(e) => {
                // Skip malformed rows rather than aborting the entire parse.
                tracing::warn!("skipping history row: {}", e);
            }
        }
    }

    Ok(results)
}

// ---------------------------------------------------------------------------
// Downloads parser
// ---------------------------------------------------------------------------

/// Parse a Chromium `History` SQLite database for download records.
///
/// Reads the `downloads` table and, when available, the `downloads_url_chains`
/// table (which holds the download URL in newer Chromium builds).
pub fn parse_chrome_downloads(
    data: &[u8],
    browser: &str,
    profile: Option<&str>,
) -> Result<Vec<BrowserDownload>, String> {
    let (conn, _tmp) = open_sqlite_from_bytes(data)?;

    // First, try to get URL mappings from downloads_url_chains (newer Chromium).
    let mut url_map: std::collections::HashMap<i64, String> = std::collections::HashMap::new();
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

    // Try querying the downloads table.  Column sets differ across versions,
    // so we use row_get_opt for optional columns.
    let mut stmt = if let Ok(s) = conn.prepare(
        "SELECT id, target_path, start_time, end_time, total_bytes, tab_url
         FROM downloads
         ORDER BY start_time DESC",
    ) {
        s
    } else {
        // Fallback: some very old Chromium versions lack end_time / tab_url.
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
            // Use downloads_url_chains URL first, then fall back to tab_url.
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
            Ok(dl) => results.push(dl),
            Err(e) => {
                tracing::warn!("skipping download row: {}", e);
            }
        }
    }

    Ok(results)
}

// ---------------------------------------------------------------------------
// Cookies parser
// ---------------------------------------------------------------------------

/// Parse a Chromium `Cookies` SQLite database.
///
/// Reads the `cookies` table.  The `value` column may contain DPAPI-encrypted
/// blobs in modern Chrome/Edge; `value_preview` will be `None` for rows that
/// appear to be ciphertext.
pub fn parse_chrome_cookies(
    data: &[u8],
    _browser: &str,
    _profile: Option<&str>,
) -> Result<Vec<BrowserCookie>, String> {
    let (conn, _tmp) = open_sqlite_from_bytes(data)?;

    let mut stmt = conn
        .prepare(
            "SELECT host_key, name, value, encrypted_value, expires_utc,
                    is_secure, is_httponly, samesite
             FROM cookies
             ORDER BY host_key",
        )
        .map_err(|e| format!("prepare cookies query: {}", e))?;

    let rows = stmt
        .query_map(params![], |row| {
            let domain: String = row.get(0)?;
            let name: String = row.get(1)?;
            // Modern Chromium encrypts the value; `value` may be empty
            // while `encrypted_value` holds the ciphertext.
            let raw_value: Option<String> = row.get(2).ok();
            let encrypted_value: Option<Vec<u8>> = row.get(3).ok();

            // Prefer plaintext value; fall back to encrypted_value as a last resort.
            let value_preview = if let Some(ref v) = raw_value {
                if v.is_empty() {
                    encrypted_value
                        .as_ref()
                        .map(|b| format!("[encrypted {} bytes]", b.len()))
                } else if is_likely_encrypted(v) {
                    Some(format!("[encrypted {} bytes]", v.len()))
                } else {
                    let preview: String = v.chars().take(128).collect();
                    Some(preview)
                }
            } else {
                encrypted_value
                    .as_ref()
                    .map(|b| format!("[encrypted {} bytes]", b.len()))
            };

            let expires_utc_raw: i64 = row.get(4).unwrap_or(0);
            let secure: bool = row.get(5).unwrap_or(false);
            let http_only: bool = row.get(6).unwrap_or(false);
            let same_site: Option<i64> = row.get(7).ok();

            Ok(BrowserCookie {
                domain,
                name,
                value_preview,
                expiry: webkit_time_to_dt(expires_utc_raw),
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
                tracing::warn!("skipping cookie row: {}", e);
            }
        }
    }

    Ok(results)
}

// ---------------------------------------------------------------------------
// Session parser (Last Session / Last Tabs JSON files)
// ---------------------------------------------------------------------------

/// Parse a Chromium session restore JSON file (`Last Session`, `Last Tabs`, or
/// `Current Session` / `Current Tabs`).
///
/// Modern Chromium (v100+) stores session data as a JSON object with a
/// `windows` array.
pub fn parse_chrome_session(data: &[u8]) -> Result<Vec<BrowserSessionTab>, String> {
    let text =
        std::str::from_utf8(data).map_err(|e| format!("session file is not valid UTF-8: {}", e))?;

    let root: serde_json::Value =
        serde_json::from_str(text).map_err(|e| format!("session JSON parse error: {}", e))?;

    let mut results = Vec::new();

    let windows = match root.get("windows") {
        Some(serde_json::Value::Array(windows)) => windows,
        _ => {
            // Some session files use a different top-level key or are plain arrays
            // of tabs.  If we don't see "windows", try a top-level array of tab
            // objects directly.
            if let Some(arr) = root.as_array() {
                for (ti, tab) in arr.iter().enumerate() {
                    if let Some(tab_result) = parse_session_tab_entry(tab, 0, ti as i32) {
                        results.push(tab_result);
                    }
                }
                return Ok(results);
            }
            return Ok(results);
        }
    };

    for window in windows {
        let window_index = window.get("index").and_then(|v| v.as_i64()).unwrap_or(0) as i32;

        let tabs = match window.get("tabs") {
            Some(serde_json::Value::Array(tabs)) => tabs,
            _ => continue,
        };

        for (ti, tab) in tabs.iter().enumerate() {
            let tab_index = tab
                .get("index")
                .and_then(|v| v.as_i64())
                .unwrap_or(ti as i64) as i32;

            if let Some(result) = parse_session_tab_entry(tab, window_index, tab_index) {
                results.push(result);
            }
        }
    }

    Ok(results)
}

/// Extract a single `BrowserSessionTab` from a JSON tab object.
fn parse_session_tab_entry(
    tab: &serde_json::Value,
    window_index: i32,
    tab_index: i32,
) -> Option<BrowserSessionTab> {
    let url = tab
        .get("url")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())?;

    let title = tab
        .get("title")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());

    // Try several common timestamp field names.
    let last_active = tab
        .get("last_active_time")
        .or_else(|| tab.get("last_navigation_time"))
        .or_else(|| tab.get("timestamp"))
        .and_then(|v| v.as_i64())
        .and_then(webkit_time_to_dt);

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

    /// Helper: create a History SQLite db on disk, read it back as bytes.
    fn make_test_history_db() -> Vec<u8> {
        let mut tmp = tempfile::NamedTempFile::new().expect("tempfile");
        {
            let conn = Connection::open(tmp.path()).expect("open db");
            conn.execute_batch(
                "CREATE TABLE urls (id INTEGER PRIMARY KEY, url TEXT, title TEXT, visit_count INTEGER DEFAULT 0);
                 CREATE TABLE visits (id INTEGER PRIMARY KEY, url INTEGER, visit_time INTEGER);
                 INSERT INTO urls VALUES (1, 'https://example.com', 'Example', 5);
                 INSERT INTO visits VALUES (1, 1, 13355619000000000);",
            )
            .expect("batch");
            // conn goes out of scope, closes the db, data is flushed to disk.
        }
        let mut buf = Vec::new();
        tmp.read_to_end(&mut buf).expect("read tmp");
        buf
    }

    /// Helper: create a History SQLite db with two visits for the same URL.
    fn make_test_history_db_two_visits() -> Vec<u8> {
        let mut tmp = tempfile::NamedTempFile::new().expect("tempfile");
        {
            let conn = Connection::open(tmp.path()).expect("open db");
            conn.execute_batch(
                "CREATE TABLE urls (id INTEGER PRIMARY KEY, url TEXT, title TEXT, visit_count INTEGER DEFAULT 0);
                 CREATE TABLE visits (id INTEGER PRIMARY KEY, url INTEGER, visit_time INTEGER);
                 INSERT INTO urls VALUES (1, 'https://example.com', 'Example', 5);
                 INSERT INTO urls VALUES (2, 'https://rust-lang.org', 'Rust', 3);
                 INSERT INTO visits VALUES (1, 1, 13355619000000000);
                 INSERT INTO visits VALUES (2, 2, 13355700000000000);",
            )
            .expect("batch");
        }
        let mut buf = Vec::new();
        tmp.read_to_end(&mut buf).expect("read tmp");
        buf
    }

    /// Helper: create a Cookies SQLite db with sample entries.
    fn make_test_cookies_db() -> Vec<u8> {
        let mut tmp = tempfile::NamedTempFile::new().expect("tempfile");
        {
            let conn = Connection::open(tmp.path()).expect("open db");
            conn.execute_batch(
                "CREATE TABLE cookies (
                    creation_utc INTEGER NOT NULL,
                    host_key TEXT NOT NULL,
                    name TEXT NOT NULL,
                    value TEXT NOT NULL,
                    encrypted_value BLOB DEFAULT '',
                    path TEXT NOT NULL,
                    expires_utc INTEGER NOT NULL,
                    is_secure INTEGER NOT NULL DEFAULT 0,
                    is_httponly INTEGER NOT NULL DEFAULT 0,
                    last_access_utc INTEGER NOT NULL,
                    has_expires INTEGER NOT NULL DEFAULT 1,
                    is_persistent INTEGER NOT NULL DEFAULT 1,
                    priority INTEGER NOT NULL DEFAULT 1,
                    samesite INTEGER NOT NULL DEFAULT -1,
                    source_scheme INTEGER NOT NULL DEFAULT 0,
                    source_port INTEGER NOT NULL DEFAULT -1,
                    is_same_party INTEGER NOT NULL DEFAULT 0,
                    last_update_utc INTEGER NOT NULL
                );
                INSERT INTO cookies VALUES
                    (13355619000000000, '.example.com', 'session', 'abc123', '',
                     '/', 13356500000000000, 1, 1, 13355619000000000, 1, 1, 1, 2, 0, -1, 0, 13355619000000000);
                INSERT INTO cookies VALUES
                    (13355700000000000, '.google.com', 'NID', 'xyz789', '',
                     '/search', 13358000000000000, 1, 0, 13355700000000000, 1, 1, 1, 0, 0, -1, 0, 13355700000000000);",
            )
            .expect("batch");
        }
        let mut buf = Vec::new();
        tmp.read_to_end(&mut buf).expect("read tmp");
        buf
    }

    /// Helper: create an empty-but-well-formed History db.
    fn make_empty_history_db() -> Vec<u8> {
        let mut tmp = tempfile::NamedTempFile::new().expect("tempfile");
        {
            let conn = Connection::open(tmp.path()).expect("open db");
            conn.execute_batch(
                "CREATE TABLE urls (id INTEGER PRIMARY KEY, url TEXT, title TEXT, visit_count INTEGER);
                 CREATE TABLE visits (id INTEGER PRIMARY KEY, url INTEGER, visit_time INTEGER);",
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
    fn parse_chrome_history_basic() {
        let db = make_test_history_db();
        let visits = parse_chrome_history(&db, "Chrome", Some("Default")).expect("parse history");
        assert_eq!(visits.len(), 1);
        assert_eq!(visits[0].url, "https://example.com");
        assert_eq!(visits[0].title.as_deref(), Some("Example"));
        assert_eq!(visits[0].visit_count, 5);
        assert!(visits[0].visit_time.is_some());
        assert_eq!(visits[0].browser, "Chrome");
        assert_eq!(visits[0].profile.as_deref(), Some("Default"));
    }

    #[test]
    fn parse_chrome_history_empty_db() {
        let db = make_empty_history_db();
        let visits = parse_chrome_history(&db, "Edge", None).expect("parse");
        assert!(visits.is_empty());
    }

    #[test]
    fn parse_chrome_history_not_a_db() {
        let result = parse_chrome_history(b"this is not sqlite", "Chrome", None);
        assert!(result.is_err());
    }

    #[test]
    fn parse_chrome_history_two_visits() {
        let db = make_test_history_db_two_visits();
        let visits =
            parse_chrome_history(&db, "Chrome", Some("Default")).expect("parse history");
        assert_eq!(visits.len(), 2);
        // ORDER BY visit_time DESC: newer visit (rust-lang.org) comes first.
        assert_eq!(visits[0].url, "https://rust-lang.org");
        assert_eq!(visits[1].url, "https://example.com");
        assert!(visits[0].visit_time.is_some());
        assert!(visits[1].visit_time.is_some());
        assert_eq!(visits[0].browser, "Chrome");
        assert_eq!(visits[1].browser, "Chrome");
    }

    // ------------------------------------------------------------------
    // Cookies
    // ------------------------------------------------------------------

    #[test]
    fn parse_chrome_cookies_basic() {
        let db = make_test_cookies_db();
        let cookies = parse_chrome_cookies(&db, "Chrome", Some("Default")).expect("parse cookies");
        assert_eq!(cookies.len(), 2);

        // ORDER BY host_key: '.example.com' < '.google.com'
        // First row: .example.com / session
        assert_eq!(cookies[0].domain, ".example.com");
        assert_eq!(cookies[0].name, "session");
        assert_eq!(cookies[0].value_preview.as_deref(), Some("abc123"));
        assert!(cookies[0].expiry.is_some());
        assert!(cookies[0].secure);
        assert!(cookies[0].http_only);
        assert_eq!(cookies[0].same_site, Some(2)); // strict

        // Second row: .google.com / NID
        assert_eq!(cookies[1].domain, ".google.com");
        assert_eq!(cookies[1].name, "NID");
        assert_eq!(cookies[1].value_preview.as_deref(), Some("xyz789"));
        assert!(cookies[1].secure);
        assert!(!cookies[1].http_only);
        assert_eq!(cookies[1].same_site, Some(0)); // none
    }

    #[test]
    fn parse_chrome_cookies_empty_db() {
        let mut tmp = tempfile::NamedTempFile::new().expect("tempfile");
        {
            let conn = Connection::open(tmp.path()).expect("open db");
            conn.execute_batch(
                "CREATE TABLE cookies (
                    creation_utc INTEGER NOT NULL,
                    host_key TEXT NOT NULL,
                    name TEXT NOT NULL,
                    value TEXT NOT NULL,
                    encrypted_value BLOB DEFAULT '',
                    path TEXT NOT NULL,
                    expires_utc INTEGER NOT NULL,
                    is_secure INTEGER NOT NULL DEFAULT 0,
                    is_httponly INTEGER NOT NULL DEFAULT 0,
                    last_access_utc INTEGER NOT NULL,
                    has_expires INTEGER NOT NULL DEFAULT 1,
                    is_persistent INTEGER NOT NULL DEFAULT 1,
                    priority INTEGER NOT NULL DEFAULT 1,
                    samesite INTEGER NOT NULL DEFAULT -1,
                    source_scheme INTEGER NOT NULL DEFAULT 0,
                    source_port INTEGER NOT NULL DEFAULT -1,
                    is_same_party INTEGER NOT NULL DEFAULT 0,
                    last_update_utc INTEGER NOT NULL
                );",
            )
            .expect("batch");
        }
        let mut buf = Vec::new();
        tmp.read_to_end(&mut buf).expect("read tmp");
        let cookies = parse_chrome_cookies(&buf, "Chrome", None).expect("parse");
        assert!(cookies.is_empty());
    }

    #[test]
    fn parse_chrome_cookies_encrypted_value() {
        let mut tmp = tempfile::NamedTempFile::new().expect("tempfile");
        {
            let conn = Connection::open(tmp.path()).expect("open db");
            conn.execute_batch(
                "CREATE TABLE cookies (
                    creation_utc INTEGER NOT NULL,
                    host_key TEXT NOT NULL,
                    name TEXT NOT NULL,
                    value TEXT NOT NULL,
                    encrypted_value BLOB DEFAULT '',
                    path TEXT NOT NULL,
                    expires_utc INTEGER NOT NULL,
                    is_secure INTEGER NOT NULL DEFAULT 0,
                    is_httponly INTEGER NOT NULL DEFAULT 0,
                    last_access_utc INTEGER NOT NULL,
                    has_expires INTEGER NOT NULL DEFAULT 1,
                    is_persistent INTEGER NOT NULL DEFAULT 1,
                    priority INTEGER NOT NULL DEFAULT 1,
                    samesite INTEGER NOT NULL DEFAULT -1,
                    source_scheme INTEGER NOT NULL DEFAULT 0,
                    source_port INTEGER NOT NULL DEFAULT -1,
                    is_same_party INTEGER NOT NULL DEFAULT 0,
                    last_update_utc INTEGER NOT NULL
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
                "INSERT INTO cookies VALUES
                    (13355619000000000, '.example.com', 'enc', ?1, '',
                     '/', 13356500000000000, 0, 0, 0, 1, 1, 1, -1, 0, -1, 0, 0)",
                rusqlite::params![enc_value],
            )
            .expect("insert");
        }
        let mut buf = Vec::new();
        tmp.read_to_end(&mut buf).expect("read tmp");
        let cookies = parse_chrome_cookies(&buf, "Chrome", None).expect("parse");
        assert_eq!(cookies.len(), 1);
        assert!(cookies[0]
            .value_preview
            .as_deref()
            .unwrap()
            .starts_with("[encrypted"));
    }

    // ------------------------------------------------------------------
    // Time conversion
    // ------------------------------------------------------------------

    #[test]
    fn webkit_time_conversion() {
        // 13355619000000000 => 2024-03-15 approximately
        let dt = webkit_time_to_dt(13355619000000000);
        assert!(dt.is_some());
        let dt = dt.unwrap();
        assert_eq!(dt.year_ce(), (true, 2024));
        assert_eq!(dt.month(), 3);
    }

    #[test]
    fn webkit_time_zero_is_none() {
        assert!(webkit_time_to_dt(0).is_none());
        assert!(webkit_time_to_dt(-1).is_none());
    }

    // ------------------------------------------------------------------
    // Session JSON
    // ------------------------------------------------------------------

    #[test]
    fn parse_chrome_session_json() {
        let json = r#"{
            "windows": [
                {
                    "index": 0,
                    "tabs": [
                        {
                            "index": 0,
                            "url": "https://example.com",
                            "title": "Example Domain",
                            "last_active_time": 13355619000000000
                        },
                        {
                            "index": 1,
                            "url": "https://openai.com",
                            "title": "OpenAI",
                            "last_active_time": 13355620000000000
                        }
                    ]
                },
                {
                    "index": 1,
                    "tabs": [
                        {
                            "index": 0,
                            "url": "https://rust-lang.org",
                            "title": null,
                            "last_navigation_time": 13355621000000000
                        }
                    ]
                }
            ]
        }"#;

        let tabs = parse_chrome_session(json.as_bytes()).expect("parse session");
        assert_eq!(tabs.len(), 3);

        assert_eq!(tabs[0].url, "https://example.com");
        assert_eq!(tabs[0].window_index, 0);
        assert_eq!(tabs[0].tab_index, 0);
        assert_eq!(tabs[0].title.as_deref(), Some("Example Domain"));

        assert_eq!(tabs[1].window_index, 0);
        assert_eq!(tabs[1].tab_index, 1);

        assert_eq!(tabs[2].url, "https://rust-lang.org");
        assert_eq!(tabs[2].window_index, 1);
        assert_eq!(tabs[2].tab_index, 0);
        assert_eq!(tabs[2].title, None);
    }

    #[test]
    fn parse_chrome_session_empty_json() {
        let tabs = parse_chrome_session(b"{}").expect("parse empty");
        assert!(tabs.is_empty());
    }

    #[test]
    fn parse_chrome_session_top_level_array() {
        let json = r#"[
            {"url": "https://a.com", "index": 0},
            {"url": "https://b.com", "index": 1}
        ]"#;
        let tabs = parse_chrome_session(json.as_bytes()).expect("parse array");
        assert_eq!(tabs.len(), 2);
        assert_eq!(tabs[0].url, "https://a.com");
        assert_eq!(tabs[0].window_index, 0);
        assert_eq!(tabs[1].url, "https://b.com");
    }

    #[test]
    fn parse_chrome_session_invalid_utf8() {
        let result = parse_chrome_session(&[0xff, 0xfe, 0x00, 0x00]);
        assert!(result.is_err());
    }

    #[test]
    fn parse_chrome_session_skips_tabs_without_url() {
        let json = r#"{
            "windows": [
                {
                    "index": 0,
                    "tabs": [
                        {"index": 0, "title": "no url here"},
                        {"index": 1, "url": "https://valid.com", "title": "Valid"}
                    ]
                }
            ]
        }"#;
        let tabs = parse_chrome_session(json.as_bytes()).expect("parse");
        assert_eq!(tabs.len(), 1);
        assert_eq!(tabs[0].url, "https://valid.com");
    }

    // ------------------------------------------------------------------
    // Encryption heuristic
    // ------------------------------------------------------------------

    #[test]
    fn is_likely_encrypted_plain_text() {
        assert!(!is_likely_encrypted("hello world"));
        assert!(!is_likely_encrypted("sessionid=abc123"));
        assert!(!is_likely_encrypted(""));
    }

    #[test]
    fn is_likely_encrypted_binary_blob() {
        // Build a string containing many non-printable bytes so that the
        // heuristic triggers (> 30 % non-printable).
        let mut raw = vec![b'a'; 20]; // 20 printable
        raw.extend(vec![0x00u8; 20]); // 20 non-printable → 50 %
        let mixed = String::from_utf8_lossy(&raw).into_owned();
        assert!(is_likely_encrypted(&mixed));
    }

    #[test]
    fn is_likely_encrypted_short_value() {
        // Values shorter than 8 bytes are never classified as encrypted.
        assert!(!is_likely_encrypted("\x00\x01\x02"));
    }
}
