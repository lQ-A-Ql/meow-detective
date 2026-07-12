use super::*;
use rusqlite::Connection;
use std::io::Read;

fn make_test_db() -> Vec<u8> {
    let mut tmp = tempfile::NamedTempFile::new().expect("tempfile");
    {
        let conn = Connection::open(tmp.path()).expect("open db");
        conn.execute_batch(
            "CREATE TABLE ZCALLRECORD (
                    Z_PK INTEGER PRIMARY KEY,
                    ZADDRESS TEXT,
                    ZDATE REAL,
                    ZDURATION INTEGER,
                    ZANSWERED INTEGER,
                    ZORIGINATED INTEGER
                );
                INSERT INTO ZCALLRECORD VALUES (1, '+15551234567', 689860800.0, 120, 1, 0);
                INSERT INTO ZCALLRECORD VALUES (2, '+15559876543', 689500800.0, 0, 0, 1);
                INSERT INTO ZCALLRECORD VALUES (3, NULL, 689508000.0, 15, 1, 0);
                INSERT INTO ZCALLRECORD VALUES (4, 'unknown_caller', NULL, 0, 0, 0);",
        )
        .expect("batch");
    }
    let mut buf = Vec::new();
    tmp.read_to_end(&mut buf).expect("read tmp");
    buf
}

fn make_empty_db() -> Vec<u8> {
    let mut tmp = tempfile::NamedTempFile::new().expect("tempfile");
    {
        let conn = Connection::open(tmp.path()).expect("open db");
        conn.execute_batch(
            "CREATE TABLE ZCALLRECORD (
                    Z_PK INTEGER PRIMARY KEY,
                    ZADDRESS TEXT,
                    ZDATE REAL,
                    ZDURATION INTEGER,
                    ZANSWERED INTEGER,
                    ZORIGINATED INTEGER
                );",
        )
        .expect("batch");
    }
    let mut buf = Vec::new();
    tmp.read_to_end(&mut buf).expect("read tmp");
    buf
}

#[test]
fn parse_call_history_basic() {
    let db = make_test_db();
    let calls = parse_call_history(&db).expect("parse calls");
    assert_eq!(calls.len(), 4);

    // ORDER BY ZDATE DESC: call 1 → call 3 → call 2 → call 4
    assert_eq!(calls[0].phone_number.as_deref(), Some("+15551234567"));
    assert_eq!(calls[0].duration_seconds, Some(120));
    assert!(!calls[0].is_outgoing); // originated=0 → incoming
    assert!(calls[0].timestamp.is_some());

    // call 3: no phone number
    assert!(calls[1].phone_number.is_none());
    assert_eq!(calls[1].duration_seconds, Some(15));
    assert!(!calls[1].is_outgoing);

    // call 2: outgoing
    assert_eq!(calls[2].phone_number.as_deref(), Some("+15559876543"));
    assert_eq!(calls[2].duration_seconds, Some(0));
    assert!(calls[2].is_outgoing);

    // call 4: no timestamp
    assert_eq!(calls[3].phone_number.as_deref(), Some("unknown_caller"));
    assert!(calls[3].timestamp.is_none());
    assert!(!calls[3].is_outgoing);
}

#[test]
fn parse_call_history_empty_db() {
    let db = make_empty_db();
    let calls = parse_call_history(&db).expect("parse");
    assert!(calls.is_empty());
}

#[test]
fn parse_call_history_not_a_db() {
    let result = parse_call_history(b"not sqlite");
    assert!(result.is_err());
}

#[test]
fn parse_call_history_outgoing_timestamps() {
    let mut tmp = tempfile::NamedTempFile::new().expect("tempfile");
    {
        let conn = Connection::open(tmp.path()).expect("open db");
        conn.execute_batch(
            "CREATE TABLE ZCALLRECORD (
                    Z_PK INTEGER PRIMARY KEY,
                    ZADDRESS TEXT,
                    ZDATE REAL,
                    ZDURATION INTEGER,
                    ZANSWERED INTEGER,
                    ZORIGINATED INTEGER
                );
                INSERT INTO ZCALLRECORD VALUES
                    (1, '+1', 689500800.0, 60, 1, 1),
                    (2, '+2', 689600800.0, 30, 1, 1),
                    (3, '+3', 689700800.0, 10, 1, 1);",
        )
        .expect("batch");
    }
    let mut buf = Vec::new();
    tmp.read_to_end(&mut buf).expect("read tmp");
    let calls = parse_call_history(&buf).expect("parse");
    assert_eq!(calls.len(), 3);
    for call in &calls {
        assert!(call.is_outgoing);
        assert!(call.timestamp.is_some());
    }
}
