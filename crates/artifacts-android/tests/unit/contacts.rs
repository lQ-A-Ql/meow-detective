use super::*;

/// Build a synthetic contacts2.db with test data.
fn build_contacts_test_db() -> Vec<u8> {
    let tmp = tempfile::Builder::new()
        .suffix(".contacts2.db")
        .tempfile()
        .expect("create temp file");
    let tmp_path = tmp.path().to_path_buf();
    drop(tmp);

    let conn = rusqlite::Connection::open(&tmp_path).expect("open temp db");

    conn.execute_batch(
        "CREATE TABLE data (
                _id INTEGER PRIMARY KEY,
                raw_contact_id INTEGER,
                mimetype TEXT,
                data1 TEXT,
                data2 TEXT,
                data3 TEXT,
                data4 TEXT
            );

            INSERT INTO data VALUES
                (1, 1, 'vnd.android.cursor.item/name', 'Alice Smith', '', '', ''),
                (2, 1, 'vnd.android.cursor.item/phone_v2', '555-0100', '2', '', ''),
                (3, 1, 'vnd.android.cursor.item/email_v2', '', '', 'alice@example.com', ''),
                (4, 1, 'vnd.android.cursor.item/organization', 'Acme Corp', '', '', ''),

                (5, 2, 'vnd.android.cursor.item/name', 'Bob Jones', '', '', ''),
                (6, 2, 'vnd.android.cursor.item/phone_v2', '555-0200', '1', '', ''),
                (7, 2, 'vnd.android.cursor.item/phone_v2', '555-0201', '2', '', ''),
                (8, 2, 'vnd.android.cursor.item/email_v2', '', '', 'bob@example.com', '');
            ",
    )
    .expect("create test db");

    drop(conn);
    std::fs::read(&tmp_path).expect("read temp db")
}

#[test]
fn parse_empty_data() {
    let result = parse_contacts(&[]);
    assert!(result.is_err());
}

#[test]
fn parse_contacts_extracts_entries() {
    let data = build_contacts_test_db();
    let contacts = parse_contacts(&data).expect("should parse");
    assert_eq!(contacts.len(), 2, "Expected 2 contacts");

    let alice = contacts
        .iter()
        .find(|c| c.display_name.contains("Alice"))
        .expect("Alice not found");
    assert_eq!(alice.display_name, "Alice Smith");
    assert_eq!(alice.phones, vec!["555-0100"]);
    assert_eq!(alice.emails, vec!["alice@example.com"]);
    assert_eq!(alice.organization.as_deref(), Some("Acme Corp"));

    let bob = contacts
        .iter()
        .find(|c| c.display_name.contains("Bob"))
        .expect("Bob not found");
    assert_eq!(bob.display_name, "Bob Jones");
    assert_eq!(bob.phones.len(), 2);
    assert!(bob.phones.contains(&"555-0200".to_string()));
    assert!(bob.phones.contains(&"555-0201".to_string()));
    assert_eq!(bob.emails, vec!["bob@example.com"]);
    assert!(bob.organization.is_none());
}

#[test]
fn parse_invalid_sqlite_handles_gracefully() {
    let result = parse_contacts(b"not a sqlite database");
    assert!(result.is_err());
}

#[test]
fn parse_contacts_empty_database() {
    let tmp = tempfile::Builder::new()
        .suffix(".empty.db")
        .tempfile()
        .expect("create temp file");
    let tmp_path = tmp.path().to_path_buf();
    drop(tmp);

    let conn = rusqlite::Connection::open(&tmp_path).expect("open temp db");
    conn.execute_batch(
        "CREATE TABLE data (
                _id INTEGER PRIMARY KEY,
                raw_contact_id INTEGER,
                mimetype TEXT,
                data1 TEXT,
                data2 TEXT,
                data3 TEXT,
                data4 TEXT
            );",
    )
    .expect("create table");
    drop(conn);

    let data = std::fs::read(&tmp_path).expect("read temp db");
    let contacts = parse_contacts(&data).expect("should parse empty");
    assert!(contacts.is_empty());
}
