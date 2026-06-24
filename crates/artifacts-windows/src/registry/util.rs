//! Shared utilities for registry parsing modules.

use chrono::{DateTime, TimeZone, Utc};

/// Convert a Windows FILETIME timestamp (100-nanosecond intervals since 1601-01-01)
/// to a `DateTime<Utc>`. Returns `None` when `ft` is zero or the value is out of range.
pub fn filetime_to_dt(ft: u64) -> Option<DateTime<Utc>> {
    if ft == 0 {
        return None;
    }
    let secs = (ft / 10_000_000) as i64 - 11_644_473_600;
    Utc.timestamp_opt(secs, ((ft % 10_000_000) * 100) as u32)
        .single()
}
