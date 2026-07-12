use chrono::{DateTime, TimeZone, Utc};

pub(super) fn firefox_time_to_dt(microseconds: i64) -> Option<DateTime<Utc>> {
    if microseconds <= 0 {
        return None;
    }
    let secs = microseconds / 1_000_000;
    let nsecs = ((microseconds % 1_000_000) * 1000) as u32;
    Utc.timestamp_opt(secs, nsecs).single()
}

pub(super) fn unix_seconds_to_dt(seconds: i64) -> Option<DateTime<Utc>> {
    if seconds <= 0 {
        return None;
    }
    Utc.timestamp_opt(seconds, 0).single()
}

pub(super) fn unix_millis_to_dt(millis: i64) -> Option<DateTime<Utc>> {
    if millis <= 0 {
        return None;
    }
    let secs = millis / 1000;
    let nsecs = ((millis % 1000) * 1_000_000) as u32;
    Utc.timestamp_opt(secs, nsecs).single()
}

pub(super) fn parse_iso_or_millis(value: &str) -> Option<DateTime<Utc>> {
    if let Ok(dt) = DateTime::parse_from_rfc3339(value) {
        return Some(dt.with_timezone(&Utc));
    }
    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S%.f") {
        return Some(Utc.from_utc_datetime(&dt));
    }
    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S") {
        return Some(Utc.from_utc_datetime(&dt));
    }
    value.parse::<i64>().ok().and_then(unix_millis_to_dt)
}
