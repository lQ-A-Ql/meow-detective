//! Parse iOS Contacts database (AddressBook.sqlitedb), extracting person records
//! with their phone numbers, email addresses, and organization.
//!
//! The AddressBook database stores person records in `ABPerson` and multi-value
//! properties (phones, emails) in `ABMultiValue` / `ABMultiValueLabel`.

use crate::{open_sqlite_from_bytes, IosArtifactError};
use serde::{Deserialize, Serialize};

/// A parsed iOS contact (address book entry).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IosContact {
    /// Full display name (First + Last, or Organization if no name set).
    pub name: String,
    /// Phone numbers associated with this contact.
    pub phones: Vec<String>,
    /// Email addresses associated with this contact.
    pub emails: Vec<String>,
    /// Organization / company name.
    pub organization: Option<String>,
}

/// Parse an iOS `AddressBook.sqlitedb` and return extracted contacts.
///
/// Queries `ABPerson` for names and organization, then joins against
/// `ABMultiValue` for phone numbers (property=3) and email addresses
/// (property=4).  Contacts without any name fields are still returned if they
/// have phone or email records.
pub fn parse_address_book(data: &[u8]) -> Result<Vec<IosContact>, IosArtifactError> {
    let (conn, _tmp) = open_sqlite_from_bytes(data)?;

    // Build a map of ROWID → (name, organization)
    let mut persons: std::collections::HashMap<i64, (String, Option<String>)> =
        std::collections::HashMap::new();
    {
        let mut stmt = conn.prepare("SELECT ROWID, First, Last, Organization FROM ABPerson")?;
        let rows = stmt.query_map([], |row| {
            let rowid: i64 = row.get(0)?;
            let first: Option<String> = row.get(1).ok();
            let last: Option<String> = row.get(2).ok();
            let org: Option<String> = row.get(3).ok();
            Ok((rowid, first, last, org))
        })?;
        for row in rows.flatten() {
            let name = match (&row.1, &row.2) {
                (Some(f), Some(l)) => format!("{} {}", f, l),
                (Some(f), None) => f.clone(),
                (None, Some(l)) => l.clone(),
                (None, None) => row.3.clone().unwrap_or_else(|| "Unknown".to_string()),
            };
            persons.insert(row.0, (name, row.3));
        }
    }

    // Collect multi-values: record_id → (property, value)
    let mut multi_values: std::collections::BTreeMap<i64, Vec<(i32, String)>> =
        std::collections::BTreeMap::new();
    if let Ok(mut stmt) = conn.prepare(
        "SELECT record_id, property, value FROM ABMultiValue ORDER BY record_id, property, identifier",
    ) {
        if let Ok(rows) = stmt.query_map([], |row| {
            let rec_id: i64 = row.get(0)?;
            let prop: i32 = row.get(1)?;
            let val: String = row.get(2)?;
            Ok((rec_id, prop, val))
        }) {
            for row in rows.flatten() {
                multi_values
                    .entry(row.0)
                    .or_default()
                    .push((row.1, row.2));
            }
        }
    }

    let mut results = Vec::new();
    for (rowid, (name, org)) in &persons {
        let mut phones = Vec::new();
        let mut emails = Vec::new();
        if let Some(vals) = multi_values.get(rowid) {
            for (prop, val) in vals {
                match prop {
                    3 => phones.push(val.clone()), // phone
                    4 => emails.push(val.clone()), // email
                    _ => {}
                }
            }
        }
        results.push(IosContact {
            name: name.clone(),
            phones,
            emails,
            organization: org.clone(),
        });
    }

    // Also include contacts that only have multi-value entries but no ABPerson row
    // (edge case for partial databases).
    for (rowid, vals) in &multi_values {
        if persons.contains_key(rowid) {
            continue;
        }
        let mut phones = Vec::new();
        let mut emails = Vec::new();
        for (prop, val) in vals {
            match prop {
                3 => phones.push(val.clone()),
                4 => emails.push(val.clone()),
                _ => {}
            }
        }
        if !phones.is_empty() || !emails.is_empty() {
            results.push(IosContact {
                name: "Unknown".to_string(),
                phones,
                emails,
                organization: None,
            });
        }
    }

    Ok(results)
}

#[cfg(test)]
mod tests {
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
}
