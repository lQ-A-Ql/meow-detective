use chrono::{DateTime, TimeZone, Utc};

/// Convert WebKit/Chrome microseconds since 1601-01-01 UTC to UTC.
pub(super) fn webkit_time_to_dt(microseconds: i64) -> Option<DateTime<Utc>> {
    if microseconds <= 0 {
        return None;
    }

    let secs = microseconds / 1_000_000 - 11_644_473_600;
    let nsecs = ((microseconds % 1_000_000) * 1000) as u32;
    Utc.timestamp_opt(secs, nsecs).single()
}
