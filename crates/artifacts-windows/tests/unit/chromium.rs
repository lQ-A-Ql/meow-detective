use super::cookies::is_likely_encrypted;
use super::time::webkit_time_to_dt;
use super::*;
use chrono::Datelike;
use rusqlite::Connection;
use std::io::Read;

fn read_temp_database(mut file: tempfile::NamedTempFile) -> Vec<u8> {
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer).expect("read temp database");
    buffer
}

fn make_test_history_db() -> Vec<u8> {
    let temp = tempfile::NamedTempFile::new().expect("tempfile");
    {
        let conn = Connection::open(temp.path()).expect("open db");
        conn.execute_batch(
            "CREATE TABLE urls (
                id INTEGER PRIMARY KEY,
                url TEXT,
                title TEXT,
                visit_count INTEGER DEFAULT 0
             );
             CREATE TABLE visits (
                id INTEGER PRIMARY KEY,
                url INTEGER,
                visit_time INTEGER
             );
             INSERT INTO urls VALUES (1, 'https://example.com', 'Example', 5);
             INSERT INTO visits VALUES (1, 1, 13355619000000000);",
        )
        .expect("batch");
    }
    read_temp_database(temp)
}

fn make_test_history_db_two_visits() -> Vec<u8> {
    let temp = tempfile::NamedTempFile::new().expect("tempfile");
    {
        let conn = Connection::open(temp.path()).expect("open db");
        conn.execute_batch(
            "CREATE TABLE urls (
                id INTEGER PRIMARY KEY,
                url TEXT,
                title TEXT,
                visit_count INTEGER DEFAULT 0
             );
             CREATE TABLE visits (
                id INTEGER PRIMARY KEY,
                url INTEGER,
                visit_time INTEGER
             );
             INSERT INTO urls VALUES (1, 'https://example.com', 'Example', 5);
             INSERT INTO urls VALUES (2, 'https://rust-lang.org', 'Rust', 3);
             INSERT INTO visits VALUES (1, 1, 13355619000000000);
             INSERT INTO visits VALUES (2, 2, 13355700000000000);",
        )
        .expect("batch");
    }
    read_temp_database(temp)
}

fn create_cookies_table(conn: &Connection) {
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
    .expect("create cookies table");
}

fn make_test_cookies_db() -> Vec<u8> {
    let temp = tempfile::NamedTempFile::new().expect("tempfile");
    {
        let conn = Connection::open(temp.path()).expect("open db");
        create_cookies_table(&conn);
        conn.execute_batch(
            "INSERT INTO cookies VALUES
                (13355619000000000, '.example.com', 'session', 'abc123', '',
                 '/', 13356500000000000, 1, 1, 13355619000000000, 1, 1, 1, 2, 0, -1, 0, 13355619000000000);
             INSERT INTO cookies VALUES
                (13355700000000000, '.google.com', 'NID', 'xyz789', '',
                 '/search', 13358000000000000, 1, 0, 13355700000000000, 1, 1, 1, 0, 0, -1, 0, 13355700000000000);",
        )
        .expect("insert cookies");
    }
    read_temp_database(temp)
}

fn make_empty_history_db() -> Vec<u8> {
    let temp = tempfile::NamedTempFile::new().expect("tempfile");
    {
        let conn = Connection::open(temp.path()).expect("open db");
        conn.execute_batch(
            "CREATE TABLE urls (
                id INTEGER PRIMARY KEY,
                url TEXT,
                title TEXT,
                visit_count INTEGER
             );
             CREATE TABLE visits (
                id INTEGER PRIMARY KEY,
                url INTEGER,
                visit_time INTEGER
             );",
        )
        .expect("batch");
    }
    read_temp_database(temp)
}

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
    let visits = parse_chrome_history(&db, "Chrome", Some("Default")).expect("parse history");
    assert_eq!(visits.len(), 2);
    assert_eq!(visits[0].url, "https://rust-lang.org");
    assert_eq!(visits[1].url, "https://example.com");
    assert!(visits[0].visit_time.is_some());
    assert!(visits[1].visit_time.is_some());
    assert_eq!(visits[0].browser, "Chrome");
    assert_eq!(visits[1].browser, "Chrome");
}

#[test]
fn parse_chrome_cookies_basic() {
    let db = make_test_cookies_db();
    let cookies = parse_chrome_cookies(&db, "Chrome", Some("Default")).expect("parse cookies");
    assert_eq!(cookies.len(), 2);

    assert_eq!(cookies[0].domain, ".example.com");
    assert_eq!(cookies[0].name, "session");
    assert_eq!(cookies[0].value_preview.as_deref(), Some("abc123"));
    assert!(cookies[0].expiry.is_some());
    assert!(cookies[0].secure);
    assert!(cookies[0].http_only);
    assert_eq!(cookies[0].same_site, Some(2));

    assert_eq!(cookies[1].domain, ".google.com");
    assert_eq!(cookies[1].name, "NID");
    assert_eq!(cookies[1].value_preview.as_deref(), Some("xyz789"));
    assert!(cookies[1].secure);
    assert!(!cookies[1].http_only);
    assert_eq!(cookies[1].same_site, Some(0));
}

#[test]
fn parse_chrome_cookies_empty_db() {
    let temp = tempfile::NamedTempFile::new().expect("tempfile");
    {
        let conn = Connection::open(temp.path()).expect("open db");
        create_cookies_table(&conn);
    }

    let cookies = parse_chrome_cookies(&read_temp_database(temp), "Chrome", None).expect("parse");
    assert!(cookies.is_empty());
}

#[test]
fn parse_chrome_cookies_encrypted_value() {
    let temp = tempfile::NamedTempFile::new().expect("tempfile");
    {
        let conn = Connection::open(temp.path()).expect("open db");
        create_cookies_table(&conn);
        let mut encrypted_value = String::from("aaaa");
        for _ in 0..12 {
            encrypted_value.push('\x01');
        }
        conn.execute(
            "INSERT INTO cookies VALUES
                (13355619000000000, '.example.com', 'enc', ?1, '',
                 '/', 13356500000000000, 0, 0, 0, 1, 1, 1, -1, 0, -1, 0, 0)",
            rusqlite::params![encrypted_value],
        )
        .expect("insert");
    }

    let cookies = parse_chrome_cookies(&read_temp_database(temp), "Chrome", None).expect("parse");
    assert_eq!(cookies.len(), 1);
    assert!(cookies[0]
        .value_preview
        .as_deref()
        .expect("value preview")
        .starts_with("[encrypted"));
}

#[test]
fn webkit_time_conversion() {
    let date_time = webkit_time_to_dt(13355619000000000).expect("valid WebKit timestamp");
    assert_eq!(date_time.year_ce(), (true, 2024));
    assert_eq!(date_time.month(), 3);
}

#[test]
fn webkit_time_zero_is_none() {
    assert!(webkit_time_to_dt(0).is_none());
    assert!(webkit_time_to_dt(-1).is_none());
}

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

#[test]
fn is_likely_encrypted_plain_text() {
    assert!(!is_likely_encrypted("hello world"));
    assert!(!is_likely_encrypted("sessionid=abc123"));
    assert!(!is_likely_encrypted(""));
}

#[test]
fn is_likely_encrypted_binary_blob() {
    let mut raw = vec![b'a'; 20];
    raw.extend(vec![0x00_u8; 20]);
    let mixed = String::from_utf8_lossy(&raw).into_owned();
    assert!(is_likely_encrypted(&mixed));
}

#[test]
fn is_likely_encrypted_short_value() {
    assert!(!is_likely_encrypted("\x00\x01\x02"));
}
