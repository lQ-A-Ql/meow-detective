use super::cookies::is_likely_encrypted;
use super::session::decompress_mozlz4;
use super::time::{firefox_time_to_dt, parse_iso_or_millis, unix_millis_to_dt, unix_seconds_to_dt};
use super::*;
use chrono::Datelike;
use rusqlite::Connection;
use std::io::Read;

fn sqlite_bytes(sql: &str) -> Vec<u8> {
    let mut tmp = tempfile::NamedTempFile::new().expect("tempfile");
    {
        let conn = Connection::open(tmp.path()).expect("open db");
        conn.execute_batch(sql).expect("batch");
    }
    let mut bytes = Vec::new();
    tmp.read_to_end(&mut bytes).expect("read tmp");
    bytes
}

fn make_firefox_places_db() -> Vec<u8> {
    sqlite_bytes(
        "CREATE TABLE moz_places (
            id INTEGER PRIMARY KEY, url TEXT, title TEXT,
            visit_count INTEGER DEFAULT 0, last_visit_date INTEGER
        );
        CREATE TABLE moz_historyvisits (
            id INTEGER PRIMARY KEY, place_id INTEGER, visit_date INTEGER
        );
        INSERT INTO moz_places VALUES
            (1, 'https://www.mozilla.org', 'Mozilla', 10, 1718352000000000);
        INSERT INTO moz_historyvisits VALUES (1, 1, 1718352000000000);
        INSERT INTO moz_historyvisits VALUES (2, 1, 1718438400000000);",
    )
}

fn make_empty_firefox_places_db() -> Vec<u8> {
    sqlite_bytes(
        "CREATE TABLE moz_places (
            id INTEGER PRIMARY KEY, url TEXT, title TEXT,
            visit_count INTEGER, last_visit_date INTEGER
        );
        CREATE TABLE moz_historyvisits (
            id INTEGER PRIMARY KEY, place_id INTEGER, visit_date INTEGER
        );",
    )
}

fn make_firefox_cookies_db() -> Vec<u8> {
    sqlite_bytes(
        "CREATE TABLE moz_cookies (
            id INTEGER PRIMARY KEY, baseDomain TEXT, name TEXT, value TEXT,
            host TEXT, path TEXT, expiry INTEGER, lastAccessed INTEGER,
            creationTime INTEGER, isSecure INTEGER, isHttpOnly INTEGER,
            sameSite INTEGER
        );
        INSERT INTO moz_cookies VALUES (
            1, 'mozilla.org', 'session', 'abc123', 'www.mozilla.org', '/',
            1718352000, 1718352000000000, 1718352000000000, 1, 1, 2
        );
        INSERT INTO moz_cookies VALUES (
            2, 'example.com', 'tracker', 'xyz789', '.example.com', '/',
            0, 1718352000000000, 1718352000000000, 0, 0, 0
        );",
    )
}

#[test]
fn parse_firefox_history_basic() {
    let visits = parse_firefox_history(&make_firefox_places_db()).expect("parse history");
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
    assert!(parse_firefox_history(&make_empty_firefox_places_db())
        .expect("parse")
        .is_empty());
}

#[test]
fn parse_firefox_history_not_a_db() {
    assert!(parse_firefox_history(b"not sqlite")
        .expect("parse")
        .is_empty());
}

#[test]
fn parse_firefox_history_no_moz_places() {
    assert!(
        parse_firefox_history(&sqlite_bytes("CREATE TABLE unrelated (id INTEGER);"))
            .expect("parse")
            .is_empty()
    );
}

#[test]
fn firefox_time_conversion() {
    let dt = firefox_time_to_dt(1718352000000000).expect("valid timestamp");
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
    let dt = unix_seconds_to_dt(1718352000).expect("valid timestamp");
    assert_eq!(dt.year_ce(), (true, 2024));
    assert_eq!(dt.month(), 6);
}

#[test]
fn unix_millis_conversion() {
    let dt = unix_millis_to_dt(1718352000000).expect("valid timestamp");
    assert_eq!(dt.year_ce(), (true, 2024));
    assert_eq!(dt.month(), 6);
}

#[test]
fn parse_iso_timestamp() {
    assert_eq!(
        parse_iso_or_millis("2024-06-14T08:00:00.000Z")
            .expect("valid timestamp")
            .year_ce(),
        (true, 2024)
    );
}

#[test]
fn parse_iso_no_timezone() {
    assert_eq!(
        parse_iso_or_millis("2024-06-14T08:00:00")
            .expect("valid timestamp")
            .year_ce(),
        (true, 2024)
    );
}

#[test]
fn parse_millis_string() {
    assert_eq!(
        parse_iso_or_millis("1718352000000")
            .expect("valid timestamp")
            .year_ce(),
        (true, 2024)
    );
}

#[test]
fn parse_firefox_downloads_json_basic() {
    let json = r#"{"list":[{
        "target":{"path":"C:\\Users\\test\\Downloads\\firefox.exe"},
        "source":{"url":"https://www.mozilla.org/firefox.exe"},
        "startTime":"2024-06-14T08:00:00.000Z",
        "endTime":"2024-06-14T08:01:00.000Z",
        "fileSize":50000000
    }]}"#;
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
}

#[test]
fn parse_firefox_downloads_json_top_level_array() {
    let downloads = parse_firefox_downloads(
        br#"[{"source":{"url":"https://a.com/a.zip"},"target":{"path":"/tmp/a.zip"}},
             {"source":{"url":"https://b.com/b.zip"},"target":{"path":"/tmp/b.zip"}}]"#,
    )
    .expect("parse");
    assert_eq!(downloads.len(), 2);
    assert_eq!(downloads[0].url, "https://a.com/a.zip");
    assert_eq!(downloads[1].url, "https://b.com/b.zip");
}

#[test]
fn parse_firefox_downloads_json_empty() {
    assert!(parse_firefox_downloads(b"{}").expect("parse").is_empty());
}

#[test]
fn parse_firefox_downloads_json_millis_timestamps() {
    let downloads = parse_firefox_downloads(
        br#"{"list":[{
            "target":{"path":"/tmp/file"},
            "source":{"url":"https://example.com/file"},
            "startTime":1718352000000,
            "endTime":1718352060000
        }]}"#,
    )
    .expect("parse");
    assert!(downloads[0].start_time.is_some());
    assert!(downloads[0].end_time.is_some());
}

#[test]
fn parse_firefox_downloads_skips_empty_entries() {
    let downloads = parse_firefox_downloads(
        br#"{"list":[
            {"source":{"url":""},"target":{}},
            {"source":{"url":"https://valid.com/file"},"target":{"path":"/tmp/valid"}}
        ]}"#,
    )
    .expect("parse");
    assert_eq!(downloads.len(), 1);
    assert_eq!(downloads[0].url, "https://valid.com/file");
}

#[test]
fn parse_firefox_cookies_basic() {
    let cookies = parse_firefox_cookies(&make_firefox_cookies_db()).expect("parse cookies");
    assert_eq!(cookies.len(), 2);
    assert_eq!(cookies[0].domain, "example.com");
    assert_eq!(cookies[0].name, "tracker");
    assert!(cookies[0].expiry.is_none());
    assert!(!cookies[0].secure);
    assert!(!cookies[0].http_only);
    assert_eq!(cookies[0].same_site, Some(0));
    assert_eq!(cookies[1].domain, "mozilla.org");
    assert_eq!(cookies[1].value_preview.as_deref(), Some("abc123"));
    assert!(cookies[1].expiry.is_some());
    assert!(cookies[1].secure);
    assert!(cookies[1].http_only);
    assert_eq!(cookies[1].same_site, Some(2));
}

#[test]
fn parse_firefox_cookies_no_table() {
    assert!(
        parse_firefox_cookies(&sqlite_bytes("CREATE TABLE unrelated (id INTEGER);"))
            .expect("parse")
            .is_empty()
    );
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
        let mut encrypted = String::from("aaaa");
        for _ in 0..12 {
            encrypted.push('\x01');
        }
        conn.execute(
            "INSERT INTO moz_cookies VALUES (
                1, 'example.com', 'enc', ?1, 'example.com', '/',
                1718352000, 0, 0, 0, 0, 0
            )",
            rusqlite::params![encrypted],
        )
        .expect("insert");
    }
    let mut bytes = Vec::new();
    tmp.read_to_end(&mut bytes).expect("read tmp");
    let cookies = parse_firefox_cookies(&bytes).expect("parse");
    assert!(cookies[0]
        .value_preview
        .as_deref()
        .expect("preview")
        .starts_with("[encrypted"));
}

fn make_mozlz4(json: &str) -> Vec<u8> {
    let raw = json.as_bytes();
    let compressed = lz4_flex::block::compress(raw);
    let mut output = Vec::with_capacity(12 + compressed.len());
    output.extend_from_slice(b"mozLz40\0");
    output.extend_from_slice(&(raw.len() as u32).to_le_bytes());
    output.extend_from_slice(&compressed);
    output
}

#[test]
fn parse_firefox_session_mozlz4_basic() {
    let compressed = make_mozlz4(
        r#"{"windows":[{"tabs":[{
            "entries":[
                {"url":"https://www.mozilla.org","title":"Mozilla"},
                {"url":"https://addons.mozilla.org","title":"Add-ons"}
            ],
            "index":2,
            "lastAccessed":1718352000000
        }]}]}"#,
    );
    let tabs = parse_firefox_session(&compressed).expect("parse session");
    assert_eq!(tabs.len(), 1);
    assert_eq!(tabs[0].url, "https://addons.mozilla.org");
    assert_eq!(tabs[0].title.as_deref(), Some("Add-ons"));
    assert_eq!(tabs[0].window_index, 0);
    assert_eq!(tabs[0].tab_index, 0);
    assert!(tabs[0].last_active.is_some());
}

#[test]
fn parse_firefox_session_plain_json() {
    let tabs = parse_firefox_session(
        br#"{"windows":[{"tabs":[{"entries":[
            {"url":"https://www.example.com","title":"Example"}
        ],"index":1}]}]}"#,
    )
    .expect("parse plain json");
    assert_eq!(tabs[0].url, "https://www.example.com");
}

#[test]
fn parse_firefox_session_multiple_windows_and_tabs() {
    let tabs = parse_firefox_session(
        br#"{"windows":[
            {"index":0,"tabs":[
                {"entries":[{"url":"https://w0t0.com"}],"index":1,"lastAccessed":1000},
                {"entries":[{"url":"https://w0t1.com"}],"index":1,"lastAccessed":2000}
            ]},
            {"index":1,"tabs":[
                {"entries":[
                    {"url":"https://w1t0.com"},
                    {"url":"https://w1t0-page2.com"}
                ],"index":2,"lastAccessed":3000}
            ]}
        ]}"#,
    )
    .expect("parse");
    assert_eq!(tabs.len(), 3);
    assert_eq!(tabs[0].url, "https://w0t0.com");
    assert_eq!(tabs[1].tab_index, 1);
    assert_eq!(tabs[2].url, "https://w1t0-page2.com");
    assert_eq!(tabs[2].window_index, 1);
}

#[test]
fn parse_firefox_session_inline_tabs() {
    let tabs = parse_firefox_session(
        br#"{"windows":[{"tabs":[{"url":"https://direct.com","title":"Direct"}]}]}"#,
    )
    .expect("parse");
    assert_eq!(tabs[0].url, "https://direct.com");
}

#[test]
fn parse_firefox_session_empty_json() {
    assert!(parse_firefox_session(b"{}").expect("parse").is_empty());
}

#[test]
fn parse_firefox_session_invalid_utf8() {
    assert!(parse_firefox_session(&[0xff, 0xfe, 0x00, 0x00]).is_err());
}

#[test]
fn parse_firefox_session_skips_entries_without_url() {
    let tabs = parse_firefox_session(
        br#"{"windows":[{"tabs":[{"entries":[
            {"title":"no url here"},
            {"url":"https://valid.com","title":"Valid"}
        ],"index":2}]}]}"#,
    )
    .expect("parse");
    assert_eq!(tabs.len(), 1);
    assert_eq!(tabs[0].url, "https://valid.com");
}

#[test]
fn mozlz4_roundtrip() {
    let original = "hello firefox session data";
    let compressed = make_mozlz4(original);
    assert_eq!(
        decompress_mozlz4(&compressed).expect("decompress"),
        original.as_bytes()
    );
}

#[test]
fn mozlz4_rejects_bad_magic() {
    assert!(decompress_mozlz4(b"not mozLz4 data!!!").is_err());
}

#[test]
fn mozlz4_rejects_short_data() {
    assert!(decompress_mozlz4(b"short").is_err());
}

#[test]
fn parse_firefox_downloads_from_sqlite_basic() {
    let bytes = sqlite_bytes(
        "CREATE TABLE moz_places (id INTEGER PRIMARY KEY, url TEXT, title TEXT);
         CREATE TABLE moz_anno_attributes (id INTEGER PRIMARY KEY, name TEXT);
         CREATE TABLE moz_annos (
             id INTEGER PRIMARY KEY, place_id INTEGER, anno_attribute_id INTEGER,
             content TEXT, flags INTEGER DEFAULT 0, expiration INTEGER DEFAULT 0,
             type INTEGER DEFAULT 3, dateAdded INTEGER, lastModified INTEGER
         );
         INSERT INTO moz_places VALUES
             (1, 'https://example.com/file.zip', 'Download page');
         INSERT INTO moz_anno_attributes VALUES
             (1, 'downloads/destinationFileURI');
         INSERT INTO moz_anno_attributes VALUES (2, 'downloads/metaData');
         INSERT INTO moz_annos VALUES
             (1, 1, 1, '/tmp/file.zip', 0, 0, 3, 1718352000000000, 1718352000000000),
             (2, 1, 2, '{}', 0, 0, 3, 1718352000000000, 1718352060000000);",
    );
    let downloads = parse_firefox_downloads(&bytes).expect("parse downloads");
    assert_eq!(downloads.len(), 1);
    assert_eq!(downloads[0].url, "https://example.com/file.zip");
    assert_eq!(downloads[0].target_path.as_deref(), Some("/tmp/file.zip"));
    assert!(downloads[0].start_time.is_some());
    assert_eq!(downloads[0].browser, "Firefox");
}

#[test]
fn parse_firefox_downloads_from_sqlite_no_tables() {
    let bytes = sqlite_bytes("CREATE TABLE unrelated (id INTEGER);");
    assert!(parse_firefox_downloads(&bytes).expect("parse").is_empty());
}

#[test]
fn is_likely_encrypted_plain_text() {
    assert!(!is_likely_encrypted("session=abc123"));
    assert!(!is_likely_encrypted(""));
}

#[test]
fn is_likely_encrypted_binary_blob() {
    let mut raw = vec![b'a'; 20];
    raw.extend(vec![0; 20]);
    assert!(is_likely_encrypted(&String::from_utf8_lossy(&raw)));
}

#[test]
fn is_likely_encrypted_short_value() {
    assert!(!is_likely_encrypted("\x00\x01\x02"));
}
