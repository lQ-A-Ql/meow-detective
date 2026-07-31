use chrono::{DateTime, Utc};

pub fn is_meaningful_timestamp(timestamp: DateTime<Utc>) -> bool {
    timestamp != DateTime::UNIX_EPOCH
}
