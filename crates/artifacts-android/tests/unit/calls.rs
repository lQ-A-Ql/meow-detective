use super::*;

fn build_calls_test_db() -> Vec<u8> {
    let tmp = tempfile::Builder::new()
        .suffix(".calllog.db")
        .tempfile()
        .expect("create temp file");
    let tmp_path = tmp.path().to_path_buf();
    drop(tmp);

    let conn = rusqlite::Connection::open(&tmp_path).expect("open temp db");

    conn.execute_batch(
        "CREATE TABLE calls (
                _id INTEGER PRIMARY KEY,
                number TEXT,
                date INTEGER,
                duration INTEGER,
                type INTEGER,
                new INTEGER DEFAULT 0,
                name TEXT,
                numbertype INTEGER,
                numberlabel TEXT
            );

            -- 2024-06-15 10:00:00 UTC = 1718445600000 ms
            INSERT INTO calls VALUES (1, '555-0100', 1718445600000, 120, 1, 1, 'Alice', 2, '');
            -- 2024-06-15 10:30:00 UTC = 1718447400000 ms
            INSERT INTO calls VALUES (2, '555-0200', 1718447400000, 45, 2, 0, 'Bob', 1, '');
            -- 2024-06-15 11:00:00 UTC = 1718449200000 ms
            INSERT INTO calls VALUES (3, '555-0300', 1718449200000, 0, 3, 1, NULL, 0, '');
            ",
    )
    .expect("create test db");

    drop(conn);
    std::fs::read(&tmp_path).expect("read temp db")
}

#[test]
fn parse_empty_data() {
    let result = parse_calls(&[]);
    assert!(result.is_err());
}

#[test]
fn parse_calls_extracts_entries() {
    let data = build_calls_test_db();
    let calls = parse_calls(&data).expect("should parse");
    assert_eq!(calls.len(), 3, "Expected 3 call records");

    // Incoming call
    let incoming = calls
        .iter()
        .find(|c| c.call_type == 1)
        .expect("incoming call not found");
    assert_eq!(incoming.number.as_deref(), Some("555-0100"));
    assert_eq!(incoming.duration_seconds, Some(120));
    assert_eq!(incoming.call_type, 1);
    assert!(incoming.date.is_some());
    assert!(incoming.date.as_ref().unwrap().starts_with("2024-06-15"));

    // Outgoing call
    let outgoing = calls
        .iter()
        .find(|c| c.call_type == 2)
        .expect("outgoing call not found");
    assert_eq!(outgoing.number.as_deref(), Some("555-0200"));
    assert_eq!(outgoing.duration_seconds, Some(45));
    assert_eq!(outgoing.call_type, 2);

    // Missed call
    let missed = calls
        .iter()
        .find(|c| c.call_type == 3)
        .expect("missed call not found");
    assert_eq!(missed.number.as_deref(), Some("555-0300"));
    assert_eq!(missed.duration_seconds, Some(0));
    assert_eq!(missed.call_type, 3);
}

#[test]
fn parse_invalid_sqlite_handles_gracefully() {
    let result = parse_calls(b"not a sqlite database");
    assert!(result.is_err());
}

#[test]
fn parse_calls_null_fields() {
    let tmp = tempfile::Builder::new()
        .suffix(".nullcalls.db")
        .tempfile()
        .expect("create temp file");
    let tmp_path = tmp.path().to_path_buf();
    drop(tmp);

    let conn = rusqlite::Connection::open(&tmp_path).expect("open temp db");
    conn.execute_batch(
            "CREATE TABLE calls (_id INTEGER PRIMARY KEY, number TEXT, date INTEGER, duration INTEGER, type INTEGER);
             INSERT INTO calls VALUES (1, NULL, 0, NULL, 1);",
        )
        .expect("create test db");
    drop(conn);

    let data = std::fs::read(&tmp_path).expect("read temp db");
    let calls = parse_calls(&data).expect("should parse");
    assert_eq!(calls.len(), 1);
    assert!(calls[0].number.is_none());
    assert!(calls[0].date.is_none());
    assert!(calls[0].duration_seconds.is_none());
    assert_eq!(calls[0].call_type, 1);
}
