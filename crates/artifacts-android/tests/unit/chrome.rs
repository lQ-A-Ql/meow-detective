use super::*;

fn build_chrome_test_db() -> Vec<u8> {
    let tmp = tempfile::Builder::new()
        .suffix(".chrome.db")
        .tempfile()
        .expect("create temp file");
    let tmp_path = tmp.path().to_path_buf();
    drop(tmp);

    let conn = rusqlite::Connection::open(&tmp_path).expect("open temp db");

    conn.execute_batch(
            "CREATE TABLE urls (
                id INTEGER PRIMARY KEY,
                url TEXT,
                title TEXT,
                visit_count INTEGER DEFAULT 0,
                typed_count INTEGER DEFAULT 0,
                last_visit_time INTEGER NOT NULL,
                hidden INTEGER DEFAULT 0
            );

            CREATE TABLE visits (
                id INTEGER PRIMARY KEY,
                url INTEGER NOT NULL,
                visit_time INTEGER NOT NULL,
                from_visit INTEGER,
                transition INTEGER DEFAULT 0,
                segment_id INTEGER,
                visit_duration INTEGER DEFAULT 0
            );

            -- 2024-06-15T12:00:00Z
            -- Unix: 1718452800 seconds → micros: 1718452800000000
            -- Chrome: 1718452800000000 + 11644473600000000 = 13362926400000000
            INSERT INTO urls VALUES (1, 'https://example.com', 'Example Domain', 5, 2, 13362926400000000, 0);
            INSERT INTO urls VALUES (2, 'https://rust-lang.org', 'Rust Programming Language', 3, 1, 13363140000000000, 0);

            INSERT INTO visits VALUES (1, 1, 13362926400000000, 0, 805306368, 0, 120000000);
            INSERT INTO visits VALUES (2, 2, 13363140000000000, 0, 805306368, 0, 300000000);
            -- Entry with zero visit_time
            INSERT INTO visits VALUES (3, 1, 0, 0, 805306368, 0, 0);
            ",
        )
        .expect("create test db");

    drop(conn);
    std::fs::read(&tmp_path).expect("read temp db")
}

#[test]
fn parse_empty_data() {
    let result = parse_chrome_history(&[]);
    assert!(result.is_err());
}

#[test]
fn parse_chrome_history_extracts_entries() {
    let data = build_chrome_test_db();
    let visits = parse_chrome_history(&data).expect("should parse");
    // 3 visit rows but one has visit_time=0 which maps to None (not excluded)
    assert_eq!(visits.len(), 3, "Expected 3 visit rows");

    // Entries with valid timestamps
    let valid: Vec<_> = visits.iter().filter(|v| v.visit_time.is_some()).collect();
    assert!(
        valid.len() >= 2,
        "Expected at least 2 valid-timestamp entries"
    );

    let example = visits
        .iter()
        .find(|v| v.url == "https://example.com" && v.visit_time.is_some())
        .expect("example.com visit not found");
    assert_eq!(example.title.as_deref(), Some("Example Domain"));
    assert!(example
        .visit_time
        .as_ref()
        .unwrap()
        .starts_with("2024-06-15"));

    let rust_visit = visits
        .iter()
        .find(|v| v.url == "https://rust-lang.org")
        .expect("rust-lang.org not found");
    assert_eq!(
        rust_visit.title.as_deref(),
        Some("Rust Programming Language")
    );
}

#[test]
fn parse_invalid_sqlite_handles_gracefully() {
    let result = parse_chrome_history(b"not a sqlite database");
    assert!(result.is_err());
}

#[test]
fn convert_chrome_timestamp_zero() {
    assert!(convert_chrome_timestamp(0).is_none());
}

#[test]
fn convert_chrome_timestamp_negative() {
    assert!(convert_chrome_timestamp(-100).is_none());
}

#[test]
fn convert_chrome_timestamp_valid() {
    // 2024-06-15T12:00:00Z in Chrome microseconds
    let unix_secs: i64 = 1718452800;
    let chrome_micros = unix_secs * 1_000_000 + NT_EPOCH_OFFSET_MICROS;
    let result = convert_chrome_timestamp(chrome_micros);
    assert!(result.is_some());
    let iso = result.unwrap();
    assert!(
        iso.starts_with("2024-06-15T12:00:00"),
        "Expected noon timestamp, got: {}",
        iso
    );
}
