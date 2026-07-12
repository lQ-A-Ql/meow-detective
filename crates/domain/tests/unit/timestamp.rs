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
    assert!(
        dt.is_some(),
        "NTFS time {} should convert successfully",
        ntfs_time
    );
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
    let timestamp = (44 << 25) | (1 << 20) | (15 << 15) | (12 << 10) | (30 << 4);
    let dt = exfat_to_datetime(timestamp, 0, 0x20).unwrap(); // 0x20 = 32 * 15 = 480 min = 8h
    assert_eq!(dt.hour(), 4); // 12:30 - 8h = 04:30
    assert_eq!(dt.minute(), 30);
}

#[test]
fn exfat_to_datetime_with_negative_offset() {
    // 2024-01-15 12:30:00 UTC-8
    let timestamp = (44 << 25) | (1 << 20) | (15 << 15) | (12 << 10) | (30 << 4);
    let dt = exfat_to_datetime(timestamp, 0, 0xE0).unwrap(); // 0xE0 = -32 * 15 = -480 min = -8h
    assert_eq!(dt.hour(), 20); // 12:30 + 8h = 20:30
    assert_eq!(dt.minute(), 30);
}

#[test]
fn exfat_to_datetime_unknown_offset() {
    let timestamp = (44 << 25) | (1 << 20) | (15 << 15) | (12 << 10) | (30 << 4);
    let dt = exfat_to_datetime(timestamp, 0, 0xFF).unwrap();
    assert_eq!(dt.hour(), 12); // treated as UTC
}

#[test]
fn exfat_to_datetime_zero() {
    assert!(exfat_to_datetime(0, 0, 0).is_none());
}
