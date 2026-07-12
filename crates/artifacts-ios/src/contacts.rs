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
#[path = "../tests/unit/contacts.rs"]
mod tests;
