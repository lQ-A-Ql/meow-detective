use super::*;
use chrono::{Datelike, Timelike};

#[test]
fn parse_file_directory_entry() {
    let mut data = [0u8; 32];
    data[0] = 0x85; // In-use, type 5 (File)
    data[1] = 0x02; // SecondaryCount = 2
    data[4] = 0x10; // FileAttributes = directory
    data[5] = 0x00;

    let entry = DirectoryEntry::parse(&data).unwrap();
    assert!(entry.is_file());
    assert!(entry.is_directory());
}

#[test]
fn parse_stream_entry() {
    let mut data = [0u8; 32];
    data[0] = 0xC0; // In-use, type 0 (Stream)
    data[3] = 10; // NameLength = 10
    data[20] = 5; // FirstCluster = 5
    data[21] = 0;
    data[22] = 0;
    data[23] = 0;

    let entry = DirectoryEntry::parse(&data).unwrap();
    assert!(entry.is_stream());
}

#[test]
fn parse_filename_entry() {
    let mut data = [0u8; 32];
    data[0] = 0xC1; // In-use, type 1 (FileName)
                    // "TEST" in UTF-16LE
    data[2] = b'T';
    data[4] = b'E';
    data[6] = b'S';
    data[8] = b'T';

    let entry = DirectoryEntry::parse(&data).unwrap();
    assert!(entry.is_filename());
    if let DirectoryEntry::FileName { name } = entry {
        assert_eq!(name, "TEST");
    }
}

#[test]
fn parse_deleted_entry() {
    let mut data = [0u8; 32];
    data[0] = 0x00; // Not in-use

    let entry = DirectoryEntry::parse(&data).unwrap();
    assert!(entry.is_deleted());
}

#[test]
fn reject_oversized_volume_label_without_panic() {
    let mut data = [0u8; 32];
    data[0] = 0x83; // In-use, type 3 (Volume Label)
    data[1] = 16; // 16 UTF-16 chars would require 32 bytes after offset 2.

    let err = DirectoryEntry::parse(&data).unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    assert!(err.to_string().contains("volume label length"));
}

#[test]
fn parse_exfat_timestamp_valid() {
    // 2024-01-15 12:30:22
    // Year: 2024-1980 = 44
    // Month: 1
    // Day: 15
    // Hour: 12
    // Minute: 30
    // Second: 22 (stored as 11 in the field, 11*2=22)
    let timestamp = (44 << 25) | (1 << 20) | (15 << 15) | (12 << 10) | (30 << 4) | 11;

    let dt = parse_exfat_timestamp(timestamp, 0, 0).unwrap();
    assert_eq!(dt.year(), 2024);
    assert_eq!(dt.month(), 1);
    assert_eq!(dt.day(), 15);
    assert_eq!(dt.hour(), 12);
    assert_eq!(dt.minute(), 30);
    assert_eq!(dt.second(), 22);
}

#[test]
fn parse_exfat_timestamp_zero() {
    assert!(parse_exfat_timestamp(0, 0, 0).is_none());
}

#[test]
fn parse_exfat_timestamp_with_positive_utc_offset() {
    // 2024-01-15 12:30:00 UTC+8 (offset = 0x20 = 32 * 15 = 480 min = 8 hours)
    let timestamp = (44 << 25) | (1 << 20) | (15 << 15) | (12 << 10) | (30 << 4);
    let utc_offset = 0x20; // UTC+8

    let dt = parse_exfat_timestamp(timestamp, 0, utc_offset).unwrap();
    // Local 12:30 UTC+8 → UTC 04:30
    assert_eq!(dt.year(), 2024);
    assert_eq!(dt.month(), 1);
    assert_eq!(dt.day(), 15);
    assert_eq!(dt.hour(), 4);
    assert_eq!(dt.minute(), 30);
}

#[test]
fn parse_exfat_timestamp_with_negative_utc_offset() {
    // 2024-01-15 12:30:00 UTC-5 (offset = 0xE0 = -32 * 15 = -480 min)
    let timestamp = (44 << 25) | (1 << 20) | (15 << 15) | (12 << 10) | (30 << 4);
    let utc_offset = 0xE0; // -480 min ≈ UTC-8

    let dt = parse_exfat_timestamp(timestamp, 0, utc_offset).unwrap();
    // Local 12:30 UTC-8 → UTC 20:30
    assert_eq!(dt.year(), 2024);
    assert_eq!(dt.hour(), 20);
    assert_eq!(dt.minute(), 30);
}

#[test]
fn parse_exfat_timestamp_unknown_offset_treated_as_utc() {
    // 2024-01-15 12:30:00 with unknown offset (0xFF)
    let timestamp = (44 << 25) | (1 << 20) | (15 << 15) | (12 << 10) | (30 << 4);
    let dt = parse_exfat_timestamp(timestamp, 0, 0xFF).unwrap();
    // Should be treated as UTC (no adjustment)
    assert_eq!(dt.hour(), 12);
    assert_eq!(dt.minute(), 30);
}

#[test]
fn parse_directory_entries_complete() {
    let mut data = Vec::new();

    // File entry
    let mut file = [0u8; 32];
    file[0] = 0x85; // In-use, type 5
    file[1] = 0x02; // 2 secondaries
    file[4] = 0x20; // Archive attribute
    data.extend_from_slice(&file);

    // Stream entry
    let mut stream = [0u8; 32];
    stream[0] = 0xC0; // In-use, type 0
    stream[3] = 4; // NameLength = 4
    stream[20] = 10; // FirstCluster = 10
    stream[24] = 100; // DataLength = 100
    data.extend_from_slice(&stream);

    // FileName entry
    let mut name = [0u8; 32];
    name[0] = 0xC1; // In-use, type 1
    name[2] = b'T';
    name[4] = b'E';
    name[6] = b'S';
    name[8] = b'T';
    data.extend_from_slice(&name);

    let entries = parse_directory_entries(&data).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].name, "TEST");
    assert_eq!(entries[0].first_cluster, 10);
    assert_eq!(entries[0].data_length, 100);
    assert!(!entries[0].is_directory());
}
