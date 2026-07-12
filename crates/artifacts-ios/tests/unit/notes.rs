use super::*;
use rusqlite::Connection;
use std::io::Read;

fn make_test_db() -> Vec<u8> {
    let mut tmp = tempfile::NamedTempFile::new().expect("tempfile");
    {
        let conn = Connection::open(tmp.path()).expect("open db");
        conn.execute_batch(
                "CREATE TABLE ZNOTE (
                    Z_PK INTEGER PRIMARY KEY,
                    ZTITLE TEXT,
                    ZSNIPPET TEXT,
                    ZCREATIONDATE REAL,
                    ZMODIFICATIONDATE REAL
                );
                INSERT INTO ZNOTE VALUES (1, 'Shopping List', 'Milk, eggs, bread...', 689500800.0, 689860800.0);
                INSERT INTO ZNOTE VALUES (2, 'Meeting Notes', 'Discuss Q4 roadmap', 689000000.0, 689700000.0);
                INSERT INTO ZNOTE VALUES (3, NULL, 'Some scratchpad content', 689200000.0, 689200000.0);
                INSERT INTO ZNOTE VALUES (4, 'Reminders', NULL, 688500000.0, NULL);",
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
            "CREATE TABLE ZNOTE (
                    Z_PK INTEGER PRIMARY KEY,
                    ZTITLE TEXT,
                    ZSNIPPET TEXT,
                    ZCREATIONDATE REAL,
                    ZMODIFICATIONDATE REAL
                );",
        )
        .expect("batch");
    }
    let mut buf = Vec::new();
    tmp.read_to_end(&mut buf).expect("read tmp");
    buf
}

#[test]
fn parse_notes_basic() {
    let db = make_test_db();
    let notes = parse_notes(&db).expect("parse notes");
    assert_eq!(notes.len(), 4);

    // ORDER BY ZMODIFICATIONDATE DESC:
    // Shopping List (689860800.0), Meeting Notes (689700000.0),
    // scratchpad (689200000.0), Reminders (NULL → treated as 0)

    assert_eq!(notes[0].title.as_deref(), Some("Shopping List"));
    assert_eq!(notes[0].snippet.as_deref(), Some("Milk, eggs, bread..."));
    assert!(notes[0].created_at.is_some());
    assert!(notes[0].modified_at.is_some());
    assert!(notes[0].modified_at.unwrap() > notes[0].created_at.unwrap());

    assert_eq!(notes[1].title.as_deref(), Some("Meeting Notes"));
    assert_eq!(notes[1].snippet.as_deref(), Some("Discuss Q4 roadmap"));

    // Note 3: no title
    assert!(notes[2].title.is_none());
    assert_eq!(notes[2].snippet.as_deref(), Some("Some scratchpad content"));

    // Note 4: no snippet, no modified_at
    assert_eq!(notes[3].title.as_deref(), Some("Reminders"));
    assert!(notes[3].snippet.is_none());
    assert!(notes[3].created_at.is_some());
    assert!(notes[3].modified_at.is_none());
}

#[test]
fn parse_notes_empty_db() {
    let db = make_empty_db();
    let notes = parse_notes(&db).expect("parse");
    assert!(notes.is_empty());
}

#[test]
fn parse_notes_not_a_db() {
    let result = parse_notes(b"not a valid database");
    assert!(result.is_err());
}

#[test]
fn parse_notes_content_only() {
    let mut tmp = tempfile::NamedTempFile::new().expect("tempfile");
    {
        let conn = Connection::open(tmp.path()).expect("open db");
        conn.execute_batch(
            "CREATE TABLE ZNOTE (
                    Z_PK INTEGER PRIMARY KEY,
                    ZTITLE TEXT,
                    ZSNIPPET TEXT,
                    ZCREATIONDATE REAL,
                    ZMODIFICATIONDATE REAL
                );
                INSERT INTO ZNOTE VALUES (1, NULL, 'Just a quick note', 689500800.0, 689500800.0);
                INSERT INTO ZNOTE VALUES (2, NULL, 'Another thought', 689600800.0, 689600800.0);",
        )
        .expect("batch");
    }
    let mut buf = Vec::new();
    tmp.read_to_end(&mut buf).expect("read tmp");
    let notes = parse_notes(&buf).expect("parse");
    assert_eq!(notes.len(), 2);
    assert!(notes[0].title.is_none());
    assert!(notes[0].snippet.is_some());
    assert!(notes[1].title.is_none());
    assert!(notes[1].snippet.is_some());
}
