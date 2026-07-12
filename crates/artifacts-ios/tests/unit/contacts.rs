use super::*;
use rusqlite::Connection;
use std::io::Read;

fn make_test_db() -> Vec<u8> {
    let mut tmp = tempfile::NamedTempFile::new().expect("tempfile");
    {
        let conn = Connection::open(tmp.path()).expect("open db");
        conn.execute_batch(
            "CREATE TABLE ABPerson (
                    ROWID INTEGER PRIMARY KEY,
                    First TEXT,
                    Last TEXT,
                    Organization TEXT
                );
                CREATE TABLE ABMultiValue (
                    record_id INTEGER,
                    property INTEGER,
                    identifier INTEGER,
                    value TEXT
                );
                INSERT INTO ABPerson VALUES (1, 'Alice', 'Smith', 'Acme Corp');
                INSERT INTO ABPerson VALUES (2, 'Bob', NULL, NULL);
                INSERT INTO ABPerson VALUES (3, NULL, 'Jones', 'Startup Inc');

                INSERT INTO ABMultiValue VALUES (1, 3, 0, '+1-555-0100');
                INSERT INTO ABMultiValue VALUES (1, 3, 1, '+1-555-0101');
                INSERT INTO ABMultiValue VALUES (1, 4, 0, 'alice@acme.com');
                INSERT INTO ABMultiValue VALUES (2, 3, 0, '+1-555-0200');
                INSERT INTO ABMultiValue VALUES (3, 4, 0, 'jones@startup.com');",
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
            "CREATE TABLE ABPerson (
                    ROWID INTEGER PRIMARY KEY,
                    First TEXT,
                    Last TEXT,
                    Organization TEXT
                );
                CREATE TABLE ABMultiValue (
                    record_id INTEGER,
                    property INTEGER,
                    identifier INTEGER,
                    value TEXT
                );",
        )
        .expect("batch");
    }
    let mut buf = Vec::new();
    tmp.read_to_end(&mut buf).expect("read tmp");
    buf
}

fn find_contact<'a>(contacts: &'a [IosContact], name: &str) -> Option<&'a IosContact> {
    contacts.iter().find(|c| c.name == name)
}

#[test]
fn parse_contacts_basic() {
    let db = make_test_db();
    let contacts = parse_address_book(&db).expect("parse contacts");
    assert_eq!(contacts.len(), 3);

    // Alice Smith: first name + last name + org + 2 phones + 1 email
    let alice = find_contact(&contacts, "Alice Smith").expect("Alice Smith not found");
    assert_eq!(alice.organization.as_deref(), Some("Acme Corp"));
    assert_eq!(alice.phones.len(), 2);
    assert!(alice.phones.contains(&"+1-555-0100".to_string()));
    assert!(alice.phones.contains(&"+1-555-0101".to_string()));
    assert_eq!(alice.emails.len(), 1);
    assert_eq!(alice.emails[0], "alice@acme.com");

    // Bob: first name only, no org, 1 phone, 0 emails
    let bob = find_contact(&contacts, "Bob").expect("Bob not found");
    assert_eq!(bob.organization, None);
    assert_eq!(bob.phones.len(), 1);
    assert!(bob.emails.is_empty());

    // Jones: last name only, org, 1 email, 0 phones
    let jones = find_contact(&contacts, "Jones").expect("Jones not found");
    assert_eq!(jones.organization.as_deref(), Some("Startup Inc"));
    assert!(jones.phones.is_empty());
    assert_eq!(jones.emails.len(), 1);
}

#[test]
fn parse_contacts_empty_db() {
    let db = make_empty_db();
    let contacts = parse_address_book(&db).expect("parse");
    assert!(contacts.is_empty());
}

#[test]
fn parse_contacts_not_a_db() {
    let result = parse_address_book(b"garbage data");
    assert!(result.is_err());
}

#[test]
fn parse_contacts_organization_only() {
    let mut tmp = tempfile::NamedTempFile::new().expect("tempfile");
    {
        let conn = Connection::open(tmp.path()).expect("open db");
        conn.execute_batch(
            "CREATE TABLE ABPerson (
                    ROWID INTEGER PRIMARY KEY,
                    First TEXT,
                    Last TEXT,
                    Organization TEXT
                );
                CREATE TABLE ABMultiValue (
                    record_id INTEGER,
                    property INTEGER,
                    identifier INTEGER,
                    value TEXT
                );
                INSERT INTO ABPerson VALUES (1, NULL, NULL, 'Unknown LLC');
                INSERT INTO ABMultiValue VALUES (1, 3, 0, '+1-555-0300');",
        )
        .expect("batch");
    }
    let mut buf = Vec::new();
    tmp.read_to_end(&mut buf).expect("read tmp");
    let contacts = parse_address_book(&buf).expect("parse");
    assert_eq!(contacts.len(), 1);
    assert_eq!(contacts[0].name, "Unknown LLC");
    assert_eq!(contacts[0].phones.len(), 1);
    assert_eq!(contacts[0].phones[0], "+1-555-0300");
}
