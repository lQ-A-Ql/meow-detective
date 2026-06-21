//! Android Contacts parser (contacts2.db).
//!
//! Parses the `contacts2.db` SQLite database found on Android devices:
//! `/data/data/com.android.providers.contacts/databases/contacts2.db`
//!
//! Key tables:
//! - `contacts` — base contact record (name_verified, display_name_source)
//! - `raw_contacts` — per-account contact rows
//! - `data` — typed contact details (phone, email, organization, etc.)
//! - `phone_lookup` — normalized phone numbers for lookup
//!
//! This parser extracts display name, phone numbers, email addresses,
//! and organization from the contacts database.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Row from the contacts data table: (mimetype, data1, data2, data3, data4)
type ContactDataRow = (String, Option<String>, Option<String>, Option<String>, Option<String>);
/// Contact data grouped by raw_contact_id
type ContactDataGroups = HashMap<i64, Vec<ContactDataRow>>;
use std::io::Write;

/// A parsed Android contact entry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AndroidContact {
    /// Display name (may be sourced from the raw_contacts or data table).
    pub display_name: String,
    /// Phone numbers associated with this contact.
    pub phones: Vec<String>,
    /// Email addresses associated with this contact.
    pub emails: Vec<String>,
    /// Organization name if set.
    pub organization: Option<String>,
}

// Android contacts2.db MIME type constants for the `data` table.
const MIMETYPE_PHONE: &str = "vnd.android.cursor.item/phone_v2";
const MIMETYPE_EMAIL: &str = "vnd.android.cursor.item/email_v2";
const MIMETYPE_ORGANIZATION: &str = "vnd.android.cursor.item/organization";
const MIMETYPE_NAME: &str = "vnd.android.cursor.item/name";

/// Parse an Android contacts2.db database from raw bytes.
///
/// Opens the SQLite database in a temp file and queries contact records.
pub fn parse_contacts(data: &[u8]) -> Result<Vec<AndroidContact>, String> {
    if data.is_empty() {
        return Err("contacts2.db data is empty".to_string());
    }

    let mut tmp = tempfile::Builder::new()
        .suffix(".contacts2.db")
        .tempfile()
        .map_err(|e| format!("Failed to create temp file: {}", e))?;
    tmp.write_all(data)
        .map_err(|e| format!("Failed to write temp file: {}", e))?;
    tmp.flush()
        .map_err(|e| format!("Failed to flush temp file: {}", e))?;

    let conn = rusqlite::Connection::open(tmp.path())
        .map_err(|e| format!("Failed to open contacts2.db: {}", e))?;

    let contacts = query_contacts(&conn)?;
    Ok(contacts)
}

/// Query contacts from the database using the `data` table aggregation.
fn query_contacts(conn: &rusqlite::Connection) -> Result<Vec<AndroidContact>, String> {
    // Query: group data rows by raw_contact_id, collect name/phone/email/org.
    // We use raw_contact_id as the grouping key since a single contact may
    // aggregate multiple raw_contacts (e.g., Google + device-local).

    let mut stmt = conn
        .prepare(
            "SELECT raw_contact_id, mimetype, data1, data2, data3, data4
             FROM data
             WHERE mimetype IN (?1, ?2, ?3, ?4)
             ORDER BY raw_contact_id",
        )
        .map_err(|e| format!("Failed to prepare contacts query: {}", e))?;

    let rows = stmt
        .query_map(
            rusqlite::params![
                MIMETYPE_NAME,
                MIMETYPE_PHONE,
                MIMETYPE_EMAIL,
                MIMETYPE_ORGANIZATION,
            ],
            |row| {
                let raw_id: i64 = row.get(0)?;
                let mimetype: String = row.get(1)?;
                let data1: Option<String> = row.get(2)?;
                let data2: Option<String> = row.get(3)?;
                let data3: Option<String> = row.get(4)?;
                let data4: Option<String> = row.get(5)?;
                Ok((raw_id, mimetype, data1, data2, data3, data4))
            },
        )
        .map_err(|e| format!("Failed to query contacts data: {}", e))?;

    // Group by raw_contact_id
    let mut groups: ContactDataGroups = HashMap::new();
    for row in rows {
        let (raw_id, mimetype, d1, d2, d3, d4) =
            row.map_err(|e| format!("Failed to read contact row: {}", e))?;
        groups
            .entry(raw_id)
            .or_default()
            .push((mimetype, d1, d2, d3, d4));
    }

    let mut contacts: Vec<AndroidContact> = Vec::new();
    for (_raw_id, items) in groups {
        let mut display_name = String::from("Unknown");
        let mut phones: Vec<String> = Vec::new();
        let mut emails: Vec<String> = Vec::new();
        let mut organization: Option<String> = None;

        for (mime, d1, _d2, d3, _d4) in items {
            match mime.as_str() {
                MIMETYPE_NAME => {
                    if let Some(name) = d1 {
                        display_name = name;
                    }
                }
                MIMETYPE_PHONE => {
                    if let Some(phone) = d1 {
                        phones.push(phone);
                    }
                }
                MIMETYPE_EMAIL => {
                    if let Some(email) = d3 {
                        emails.push(email);
                    }
                }
                MIMETYPE_ORGANIZATION => {
                    organization = d1;
                }
                _ => {}
            }
        }

        contacts.push(AndroidContact {
            display_name,
            phones,
            emails,
            organization,
        });
    }

    Ok(contacts)
}

#[cfg(test)]
mod tests {
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
}
