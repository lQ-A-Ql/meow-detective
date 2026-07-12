use super::*;
use rusqlite::Connection;
use std::io::Read;

fn make_test_db() -> Vec<u8> {
    let mut tmp = tempfile::NamedTempFile::new().expect("tempfile");
    {
        let conn = Connection::open(tmp.path()).expect("open db");
        conn.execute_batch(
                "CREATE TABLE handle (
                    ROWID INTEGER PRIMARY KEY,
                    id TEXT
                );
                CREATE TABLE message (
                    ROWID INTEGER PRIMARY KEY,
                    text TEXT,
                    date INTEGER,
                    is_from_me INTEGER,
                    handle_id INTEGER
                );
                CREATE TABLE chat (
                    ROWID INTEGER PRIMARY KEY
                );
                CREATE TABLE chat_handle_join (
                    chat_id INTEGER,
                    handle_id INTEGER
                );
                CREATE TABLE chat_message_join (
                    chat_id INTEGER,
                    message_id INTEGER
                );

                -- Handles
                INSERT INTO handle VALUES (1, '+15551234567');
                INSERT INTO handle VALUES (2, '+15559876543');
                INSERT INTO handle VALUES (3, 'friend@example.com');

                -- Chat 100
                INSERT INTO chat VALUES (100);
                INSERT INTO chat_handle_join VALUES (100, 1);
                INSERT INTO chat_handle_join VALUES (100, 2);

                -- Chat 101
                INSERT INTO chat VALUES (101);
                INSERT INTO chat_handle_join VALUES (101, 1);
                INSERT INTO chat_handle_join VALUES (101, 3);

                -- Messages
                INSERT INTO message VALUES (1, 'Hey!', 689500800, 1, 2);      -- from me to handle 2
                INSERT INTO message VALUES (2, 'Hi there', 689504400, 0, 3);  -- from handle 3 to me
                INSERT INTO message VALUES (3, 'Meeting at 3?', 689508000, 1, 1); -- from me to handle 1
                INSERT INTO chat_message_join VALUES (100, 1);
                INSERT INTO chat_message_join VALUES (101, 2);
                INSERT INTO chat_message_join VALUES (100, 3);",
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
                "CREATE TABLE handle (ROWID INTEGER PRIMARY KEY, id TEXT);
                 CREATE TABLE message (ROWID INTEGER PRIMARY KEY, text TEXT, date INTEGER, is_from_me INTEGER, handle_id INTEGER);
                 CREATE TABLE chat (ROWID INTEGER PRIMARY KEY);
                 CREATE TABLE chat_handle_join (chat_id INTEGER, handle_id INTEGER);
                 CREATE TABLE chat_message_join (chat_id INTEGER, message_id INTEGER);",
            )
            .expect("batch");
    }
    let mut buf = Vec::new();
    tmp.read_to_end(&mut buf).expect("read tmp");
    buf
}

#[test]
fn parse_messages_basic() {
    let db = make_test_db();
    let messages = parse_messages(&db).expect("parse messages");
    assert_eq!(messages.len(), 3);

    // ORDER BY date DESC → message 3 (latest), 2, 1
    assert_eq!(messages[0].text.as_deref(), Some("Meeting at 3?"));
    assert!(messages[0].is_from_me);
    assert_eq!(messages[0].sender.as_deref(), Some("+15551234567"));

    assert_eq!(messages[1].text.as_deref(), Some("Hi there"));
    assert!(!messages[1].is_from_me);
    assert_eq!(messages[1].sender.as_deref(), Some("friend@example.com"));

    assert_eq!(messages[2].text.as_deref(), Some("Hey!"));
    assert!(messages[2].is_from_me);
    assert_eq!(messages[2].sender.as_deref(), Some("+15559876543"));

    // Timestamps should be present
    assert!(messages[0].timestamp.is_some());
    assert!(messages[1].timestamp.is_some());
    assert!(messages[2].timestamp.is_some());
}

#[test]
fn parse_messages_empty_db() {
    let db = make_empty_db();
    let messages = parse_messages(&db).expect("parse");
    assert!(messages.is_empty());
}

#[test]
fn parse_messages_not_a_db() {
    let result = parse_messages(b"not a database");
    assert!(result.is_err());
}

#[test]
fn parse_messages_no_handles() {
    let mut tmp = tempfile::NamedTempFile::new().expect("tempfile");
    {
        let conn = Connection::open(tmp.path()).expect("open db");
        conn.execute_batch(
                "CREATE TABLE handle (ROWID INTEGER PRIMARY KEY, id TEXT);
                 CREATE TABLE message (ROWID INTEGER PRIMARY KEY, text TEXT, date INTEGER, is_from_me INTEGER, handle_id INTEGER);
                 INSERT INTO message VALUES (1, 'orphan message', 689500800, 1, NULL);",
            )
            .expect("batch");
    }
    let mut buf = Vec::new();
    tmp.read_to_end(&mut buf).expect("read tmp");
    let messages = parse_messages(&buf).expect("parse");
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].text.as_deref(), Some("orphan message"));
    assert!(messages[0].sender.is_none());
    assert!(messages[0].recipients.is_empty());
}

#[test]
fn parse_messages_coredata_timestamp() {
    let mut tmp = tempfile::NamedTempFile::new().expect("tempfile");
    {
        let conn = Connection::open(tmp.path()).expect("open db");
        conn.execute_batch(
                "CREATE TABLE handle (ROWID INTEGER PRIMARY KEY, id TEXT);
                 CREATE TABLE message (ROWID INTEGER PRIMARY KEY, text TEXT, date INTEGER, is_from_me INTEGER, handle_id INTEGER);
                 -- CoreData nanosecond timestamp (~2024-11-01 in ns since 2001)
                 INSERT INTO message VALUES (1, 'coredata msg', 753000000000000000, 1, NULL);",
            )
            .expect("batch");
    }
    let mut buf = Vec::new();
    tmp.read_to_end(&mut buf).expect("read tmp");
    let messages = parse_messages(&buf).expect("parse");
    assert_eq!(messages.len(), 1);
    // Large nanosecond value should produce a valid future timestamp
    assert!(messages[0].timestamp.is_some());
}
