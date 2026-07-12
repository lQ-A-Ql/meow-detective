//! Unified timestamp validation and conversion utilities.
//!
//! Provides consistent timestamp handling across all parsers and formats.
//! All timestamps should be validated through this module to ensure consistency.

use chrono::{DateTime, Datelike, NaiveDate, TimeZone, Utc};

/// Minimum valid year for forensic timestamps (1970 = Unix epoch).
const MIN_YEAR: i32 = 1970;

/// Maximum valid year for forensic timestamps (2100).
const MAX_YEAR: i32 = 2100;

/// Validate a `DateTime<Utc>` timestamp.
///
/// Rejects timestamps outside the range [1970, 2100].
/// Returns `Some(dt)` if valid, `None` if out of range.
///
/// # Example
/// ```
/// use chrono::{Datelike, TimeZone, Utc};
/// use domain::timestamp::validate_timestamp;
///
/// let valid = Utc.timestamp_opt(1609459200, 0).single().unwrap(); // 2021-01-01
/// assert!(validate_timestamp(valid).is_some());
/// ```
pub fn validate_timestamp(dt: DateTime<Utc>) -> Option<DateTime<Utc>> {
    let year = dt.year();
    if (MIN_YEAR..=MAX_YEAR).contains(&year) {
        Some(dt)
    } else {
        None
    }
}

/// Convert a Windows FILETIME to `DateTime<Utc>`.
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
/// use chrono::Datelike;
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

/// Convert an NTFS timestamp to `DateTime<Utc>`.
///
/// NTFS timestamps are identical to FILETIME (100-nanosecond intervals
/// since 1601-01-01).
pub fn ntfs_time_to_datetime(ntfs_time: u64) -> Option<DateTime<Utc>> {
    filetime_to_datetime(ntfs_time)
}

/// Convert a Unix timestamp (seconds since 1970-01-01) to `DateTime<Utc>`.
///
/// Returns `None` if the timestamp is out of valid range.
///
/// # Example
/// ```
/// use chrono::Datelike;
/// use domain::timestamp::unix_to_datetime;
///
/// let dt = unix_to_datetime(1609459200).unwrap(); // 2021-01-01 00:00:00
/// assert_eq!(dt.year(), 2021);
/// ```
pub fn unix_to_datetime(secs: i64) -> Option<DateTime<Utc>> {
    let dt = Utc.timestamp_opt(secs, 0).single()?;
    validate_timestamp(dt)
}

/// Convert an exFAT timestamp to `DateTime<Utc>`.
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

    let naive = NaiveDate::from_ymd_opt(year, month, day)?.and_hms_milli_opt(
        hour,
        minute,
        second,
        increment_10ms as u32 * 10,
    )?;

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
#[path = "../tests/unit/timestamp.rs"]
mod tests;
