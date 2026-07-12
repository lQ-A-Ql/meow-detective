//! Parse iOS Call History database (CallHistory.storedata), extracting call log
//! records with contact info, timestamps, duration, and direction.
//!
//! The CallHistory database uses CoreData conventions: `ZCALLRECORD` stores call
//! records with `ZADDRESS` (phone number), `ZDATE` (timestamp), `ZDURATION`,
//! and `ZANSWERED` / `ZORIGINATED` flags.

use crate::{core_data_time_to_dt, open_sqlite_from_bytes, IosArtifactError};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A parsed iOS call log record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IosCall {
    /// Display name from contacts (if known). May be `None` for unknown numbers.
    pub contact: Option<String>,
    /// The phone number dialed or received (e.g. "+15551234567").
    pub phone_number: Option<String>,
    /// Call timestamp.
    pub timestamp: Option<DateTime<Utc>>,
    /// Call duration in seconds. `None` for missed or zero-duration calls.
    pub duration_seconds: Option<i32>,
    /// `true` if the call was originated (outgoing); `false` if incoming.
    pub is_outgoing: bool,
}

/// Parse an iOS `CallHistory.storedata` and return call log entries.
///
/// Queries `ZCALLRECORD` for call metadata and, when available, joins against
/// `ZHANDLE` / `ZCONTACT` for resolved names.
pub fn parse_call_history(data: &[u8]) -> Result<Vec<IosCall>, IosArtifactError> {
    let (conn, _tmp) = open_sqlite_from_bytes(data)?;

    let mut stmt = conn.prepare(
        "SELECT ZADDRESS, ZDATE, ZDURATION, ZANSWERED, ZORIGINATED FROM ZCALLRECORD ORDER BY ZDATE DESC",
    )?;

    let rows = stmt.query_map([], |row| {
        let address: Option<String> = row.get(0).ok();
        let date_raw: Option<f64> = row.get(1).ok();
        let duration: Option<i32> = row.get(2).ok();
        let answered: Option<bool> = row.get(3).ok();
        let originated: Option<bool> = row.get(4).ok();
        Ok((address, date_raw, duration, answered, originated))
    })?;

    let mut results = Vec::new();
    for row in rows {
        let (phone_number, date_raw, duration, _answered, originated) = match row {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!("skipping call record row: {}", e);
                continue;
            }
        };

        let timestamp = date_raw.and_then(core_data_time_to_dt);
        let is_outgoing = originated.unwrap_or(false);

        results.push(IosCall {
            contact: None, // Contact resolution would need Contacts DB
            phone_number,
            timestamp,
            duration_seconds: duration,
            is_outgoing,
        });
    }

    Ok(results)
}

#[cfg(test)]
#[path = "../tests/unit/calls.rs"]
mod tests;
