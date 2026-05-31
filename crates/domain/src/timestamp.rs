//! Unified timestamp validation and conversion utilities.
//!
//! Provides consistent timestamp handling across all parsers and formats.
//! All timestamps should be validated through this module to ensure consistency.

use chrono::{DateTime, Datelike, NaiveDate, TimeZone, Utc};

/// Minimum valid year for forensic timestamps (1970 = Unix epoch).
const MIN_YEAR: i32 = 1970;

/// Maximum valid year for forensic timestamps (2100).
const MAX_YEAR: i32 = 2100;

/// Validate a DateTime<Utc> timestamp.
///
/// Rejects timestamps outside the range [1970, 2100].
/// Returns `Some(dt)` if valid, `None` if out of range.
///
/// # Example
/// ```
/// use chrono::{TimeZone, Utc};
/// use domain::timestamp::validate_timestamp;
///
/// let valid = Utc.timestamp_opt(1609459200, 0).single().unwrap(); // 2021-01-01
/// assert!(validate_timestamp(valid).is_some());
/// ```
pub fn validate_timestamp(dt: DateTime<Utc>) -> Option<DateTime<Utc>> {
    let year = dt.year();
    if year >= MIN_YEAR && year <= MAX_YEAR {
        Some(dt)
    } else {
        None
    }
}

/// Convert a Windows FILETIME to DateTime<Utc>.
///
/// FILETIME is a 64-bit value representing the number of 100-nanosecond
/// intervals since January 1, 1601 (UTC).
///
/// Returns `None` if:
/// - The value is 0 (unset)
/// - The high bit is set (invalid)
/// - The conversion fails
///
/// # Example
/// ```
/// use domain::timestamp::filetime_to_datetime;
///
/// // FILETIME for 2021-01-01 00:00:00 UTC
/// let ft: u64 = 132562608000000000;
/// let dt = filetime_to_datetime(ft).unwrap();
/// assert_eq!(dt.year(), 2021);
/// ```
pub fn filetime_to_datetime(ft: u64) -> Option<DateTime<Utc>> {
    if ft == 0 || ft >= 0x8000000000000000 {
        return None;
    }

    // Convert from 100-nanosecond intervals since 1601-01-01
    // to seconds since 1970-01-01 (Unix epoch)
    // EPOCH_DIFF = 134774 days * 86400 seconds * 10_000_000 (100ns intervals)
    const EPOCH_DIFF: u64 = 116_444_736_000_000_000;

    let unix_100ns = ft.checked_sub(EPOCH_DIFF)?;
    let unix_secs = (unix_100ns / 10_000_000) as i64;
    let unix_nanos = ((unix_100ns % 10_000_000) * 100) as u32;

    let dt = Utc.timestamp_opt(unix_secs, unix_nanos).single()?;
    validate_timestamp(dt)
}

/// Convert an NTFS timestamp to DateTime<Utc>.
///
/// NTFS timestamps are identical to FILETIME (100-nanosecond intervals
/// since 1601-01-01).
pub fn ntfs_time_to_datetime(ntfs_time: u64) -> Option<DateTime<Utc>> {
    filetime_to_datetime(ntfs_time)
}

/// Convert a Unix timestamp (seconds since 1970-01-01) to DateTime<Utc>.
///
/// Returns `None` if the timestamp is out of valid range.
///
/// # Example
/// ```
/// use domain::timestamp::unix_to_datetime;
///
/// let dt = unix_to_datetime(1609459200).unwrap(); // 2021-01-01 00:00:00
/// assert_eq!(dt.year(), 2021);
/// ```
pub fn unix_to_datetime(secs: i64) -> Option<DateTime<Utc>> {
    let dt = Utc.timestamp_opt(secs, 0).single()?;
    validate_timestamp(dt)
}

/// Convert an exFAT timestamp to DateTime<Utc>.
///
/// exFAT timestamps are 32-bit values with:
/// - Bits 31-25: Year (0-127, add 1980)
/// - Bits 24-20: Month (1-12)
/// - Bits 19-15: Day (1-31)
/// - Bits 14-10: Hour (0-23)
/// - Bits 9-4: Minute (0-59)
/// - Bits 3-0: 2-second increment (0-29)
///
/// `increment_10ms` adds 0-199 milliseconds (in 10ms increments).
/// `utc_offset` is in 15-minute increments from UTC (-48 to +48).
/// 0xFF means unknown offset (treat as UTC).
pub fn exfat_to_datetime(
    timestamp: u32,
    increment_10ms: u8,
    utc_offset: u8,
) -> Option<DateTime<Utc>> {
    if timestamp == 0 {
        return None;
    }

    let year = ((timestamp >> 25) & 0x7F) as i32 + 1980;
    let month = (timestamp >> 20) & 0x1F;
    let day = (timestamp >> 15) & 0x1F;
    let hour = (timestamp >> 10) & 0x1F;
    let minute = (timestamp >> 4) & 0x3F;
    let second = (timestamp & 0x0F) * 2;

    // Validate ranges
    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour > 23
        || minute > 59
        || second > 59
    {
        return None;
    }

    let naive = NaiveDate::from_ymd_opt(year, month, day)?
        .and_hms_milli_opt(hour, minute, second, increment_10ms as u32 * 10)?;

    // Apply UTC offset
    // 0xFF = unknown (treat as UTC)
    // 0x01..0xDF = positive offset (+15 to +3435 minutes)
    // 0xE0..0xFE = negative offset (-480 to -15 minutes)
    let offset_minutes: i64 = if utc_offset == 0xFF {
        0
    } else if utc_offset <= 0xDF {
        (utc_offset as i64) * 15
    } else {
        ((utc_offset as i64) - 256) * 15
    };

    let utc_naive = naive - chrono::Duration::minutes(offset_minutes);
    let dt = utc_naive.and_utc();
    validate_timestamp(dt)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Timelike;

    #[test]
    fn validate_timestamp_valid() {
        let dt = Utc.timestamp_opt(1609459200, 0).single().unwrap(); // 2021-01-01
        assert!(validate_timestamp(dt).is_some());
    }

    #[test]
    fn validate_timestamp_too_old() {
        let dt = Utc.timestamp_opt(-1, 0).single().unwrap(); // 1969
        assert!(validate_timestamp(dt).is_none());
    }

    #[test]
    fn validate_timestamp_too_far_future() {
        // 2101-01-01 00:00:00 UTC - should be rejected
        let dt = Utc.timestamp_opt(4133980800, 0).single().unwrap();
        assert!(validate_timestamp(dt).is_none());
    }

    #[test]
    fn filetime_to_datetime_valid() {
        // FILETIME for 2021-01-01 00:00:00 UTC
        // Unix timestamp 1609459200 * 10_000_000 + EPOCH_DIFF
        let ft: u64 = 132_539_328_000_000_000;
        let dt = filetime_to_datetime(ft);
        assert!(dt.is_some(), "FILETIME {} should convert successfully", ft);
        let dt = dt.unwrap();
        assert_eq!(dt.year(), 2021);
        assert_eq!(dt.month(), 1);
        assert_eq!(dt.day(), 1);
    }

    #[test]
    fn filetime_to_datetime_zero() {
        assert!(filetime_to_datetime(0).is_none());
    }

    #[test]
    fn filetime_to_datetime_high_bit_set() {
        assert!(filetime_to_datetime(0x8000000000000000).is_none());
    }

    #[test]
    fn ntfs_time_to_datetime_valid() {
        // NTFS time (same as FILETIME) for 2021-01-01 00:00:00 UTC
        let ntfs_time: u64 = 132_539_328_000_000_000;
        let dt = ntfs_time_to_datetime(ntfs_time);
        assert!(dt.is_some(), "NTFS time {} should convert successfully", ntfs_time);
        let dt = dt.unwrap();
        assert_eq!(dt.year(), 2021);
    }

    #[test]
    fn ntfs_time_to_datetime_zero() {
        assert!(ntfs_time_to_datetime(0).is_none());
    }

    #[test]
    fn unix_to_datetime_valid() {
        let dt = unix_to_datetime(1609459200).unwrap(); // 2021-01-01
        assert_eq!(dt.year(), 2021);
    }

    #[test]
    fn exfat_to_datetime_valid() {
        // 2024-01-15 12:30:22 UTC
        let timestamp = (44 << 25) | (1 << 20) | (15 << 15) | (12 << 10) | (30 << 4) | 11;
        let dt = exfat_to_datetime(timestamp, 0, 0).unwrap();
        assert_eq!(dt.year(), 2024);
        assert_eq!(dt.month(), 1);
        assert_eq!(dt.day(), 15);
        assert_eq!(dt.hour(), 12);
        assert_eq!(dt.minute(), 30);
        assert_eq!(dt.second(), 22);
    }

    #[test]
    fn exfat_to_datetime_with_positive_offset() {
        // 2024-01-15 12:30:00 UTC+8
        let timestamp = (44 << 25) | (1 << 20) | (15 << 15) | (12 << 10) | (30 << 4) | 0;
        let dt = exfat_to_datetime(timestamp, 0, 0x20).unwrap(); // 0x20 = 32 * 15 = 480 min = 8h
        assert_eq!(dt.hour(), 4); // 12:30 - 8h = 04:30
        assert_eq!(dt.minute(), 30);
    }

    #[test]
    fn exfat_to_datetime_with_negative_offset() {
        // 2024-01-15 12:30:00 UTC-8
        let timestamp = (44 << 25) | (1 << 20) | (15 << 15) | (12 << 10) | (30 << 4) | 0;
        let dt = exfat_to_datetime(timestamp, 0, 0xE0).unwrap(); // 0xE0 = -32 * 15 = -480 min = -8h
        assert_eq!(dt.hour(), 20); // 12:30 + 8h = 20:30
        assert_eq!(dt.minute(), 30);
    }

    #[test]
    fn exfat_to_datetime_unknown_offset() {
        let timestamp = (44 << 25) | (1 << 20) | (15 << 15) | (12 << 10) | (30 << 4) | 0;
        let dt = exfat_to_datetime(timestamp, 0, 0xFF).unwrap();
        assert_eq!(dt.hour(), 12); // treated as UTC
    }

    #[test]
    fn exfat_to_datetime_zero() {
        assert!(exfat_to_datetime(0, 0, 0).is_none());
    }
}
