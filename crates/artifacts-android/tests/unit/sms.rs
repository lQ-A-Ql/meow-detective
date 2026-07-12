use super::*;

fn build_sms_test_db() -> Vec<u8> {
    let tmp = tempfile::Builder::new()
        .suffix(".mmssms.db")
        .tempfile()
        .expect("create temp file");
    let tmp_path = tmp.path().to_path_buf();
    drop(tmp);

    let conn = rusqlite::Connection::open(&tmp_path).expect("open temp db");

    conn.execute_batch(
        "CREATE TABLE sms (
                _id INTEGER PRIMARY KEY,
                address TEXT,
                body TEXT,
                date INTEGER,
                type INTEGER,
                read INTEGER DEFAULT 0
            );

            -- 2024-06-15 12:00:00 UTC = 1718452800000 ms
            INSERT INTO sms VALUES (1, '555-0100', 'Hey, how are you?', 1718452800000, 1, 1);
            -- 2024-06-15 12:05:00 UTC = 1718453100000 ms
            INSERT INTO sms VALUES (2, '555-0200', 'I''m fine, thanks!', 1718453100000, 2, 1);
            -- 2024-06-15 12:10:00 UTC = 1718453400000 ms
            INSERT INTO sms VALUES (3, '555-0300', 'Draft message', 1718453400000, 3, 0);
            ",
    )
    .expect("create test db");

    drop(conn);
    std::fs::read(&tmp_path).expect("read temp db")
}

#[test]
fn parse_empty_data() {
    let result = parse_sms(&[]);
    assert!(result.is_err());
}

#[test]
fn parse_sms_extracts_entries() {
    let data = build_sms_test_db();
    let msgs = parse_sms(&data).expect("should parse");
    assert_eq!(msgs.len(), 3, "Expected 3 SMS messages");

    // Received message
    let received = msgs
        .iter()
        .find(|m| m.sms_type == 1)
        .expect("received not found");
    assert_eq!(received.address.as_deref(), Some("555-0100"));
    assert_eq!(received.body.as_deref(), Some("Hey, how are you?"));
    assert_eq!(received.sms_type, 1);
    assert!(received.date.is_some());
    assert!(received.date.as_ref().unwrap().starts_with("2024-06-15"));

    // Sent message
    let sent = msgs
        .iter()
        .find(|m| m.sms_type == 2)
        .expect("sent not found");
    assert_eq!(sent.address.as_deref(), Some("555-0200"));
    assert_eq!(sent.body.as_deref(), Some("I'm fine, thanks!"));
    assert_eq!(sent.sms_type, 2);

    // Draft message
    let draft = msgs
        .iter()
        .find(|m| m.sms_type == 3)
        .expect("draft not found");
    assert_eq!(draft.sms_type, 3);
}

#[test]
fn parse_invalid_sqlite_handles_gracefully() {
    let result = parse_sms(b"not a sqlite database");
    assert!(result.is_err());
}

#[test]
fn parse_sms_with_null_fields() {
    let tmp = tempfile::Builder::new()
        .suffix(".nullsms.db")
        .tempfile()
        .expect("create temp file");
    let tmp_path = tmp.path().to_path_buf();
    drop(tmp);

    let conn = rusqlite::Connection::open(&tmp_path).expect("open temp db");
    conn.execute_batch(
            "CREATE TABLE sms (_id INTEGER PRIMARY KEY, address TEXT, body TEXT, date INTEGER, type INTEGER, read INTEGER DEFAULT 0);
             INSERT INTO sms VALUES (1, NULL, NULL, NULL, 1, 0);",
        )
        .expect("create test db");
    drop(conn);

    let data = std::fs::read(&tmp_path).expect("read temp db");
    let msgs = parse_sms(&data).expect("should parse");
    assert_eq!(msgs.len(), 1);
    assert!(msgs[0].address.is_none());
    assert!(msgs[0].body.is_none());
    assert!(msgs[0].date.is_none());
}

#[test]
fn parse_sms_zero_date_returns_none() {
    let tmp = tempfile::Builder::new()
        .suffix(".zerodate.db")
        .tempfile()
        .expect("create temp file");
    let tmp_path = tmp.path().to_path_buf();
    drop(tmp);

    let conn = rusqlite::Connection::open(&tmp_path).expect("open temp db");
    conn.execute_batch(
            "CREATE TABLE sms (_id INTEGER PRIMARY KEY, address TEXT, body TEXT, date INTEGER, type INTEGER, read INTEGER DEFAULT 0);
             INSERT INTO sms VALUES (1, '555-0000', 'zero date', 0, 1, 0);",
        )
        .expect("create test db");
    drop(conn);

    let data = std::fs::read(&tmp_path).expect("read temp db");
    let msgs = parse_sms(&data).expect("should parse");
    assert_eq!(msgs.len(), 1);
    assert!(msgs[0].date.is_none());
}
