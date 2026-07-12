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
type ContactDataRow = (
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
);
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
#[path = "../tests/unit/contacts.rs"]
mod tests;
