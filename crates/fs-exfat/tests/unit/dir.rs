use super::*;
use crate::time::parse_exfat_date_time;
use chrono::{DateTime, Datelike, Timelike, Utc};

fn parse_exfat_timestamp(
    timestamp: u32,
    increment_10ms: u8,
    utc_offset: u8,
) -> Option<DateTime<Utc>> {
    if timestamp == 0 {
        return None;
    }
    let date = (((timestamp >> 25) & 0x7F) << 9)
        | (((timestamp >> 20) & 0x1F) << 5)
        | ((timestamp >> 15) & 0x1F);
    let time =
        (((timestamp >> 10) & 0x1F) << 11) | (((timestamp >> 4) & 0x3F) << 5) | (timestamp & 0x0F);
    parse_exfat_date_time(date as u16, time as u16, increment_10ms, utc_offset)
}

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
fn parse_file_entry_uses_exfat_date_and_time_fields() {
    let mut data = [0u8; 32];
    data[0] = 0x85;
    let date = ((2024 - 1980) << 9) | (1 << 5) | 15;
    let time = (12 << 11) | (30 << 5) | 11;
    data[8..10].copy_from_slice(&(date as u16).to_le_bytes());
    data[10..12].copy_from_slice(&(time as u16).to_le_bytes());
    data[16..18].copy_from_slice(&(date as u16).to_le_bytes());
    data[18..20].copy_from_slice(&(time as u16).to_le_bytes());

    let DirectoryEntry::File {
        create_timestamp,
        accessed_timestamp,
        ..
    } = DirectoryEntry::parse(&data).unwrap()
    else {
        panic!("expected file entry");
    };
    let created = create_timestamp.unwrap();
    let accessed = accessed_timestamp.unwrap();
    assert_eq!(
        (created.year(), created.month(), created.day()),
        (2024, 1, 15)
    );
    assert_eq!(
        (created.hour(), created.minute(), created.second()),
        (12, 30, 22)
    );
    assert_eq!(accessed, created);
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

#[test]
fn file_entry_attributes_expose_read_only_hidden_system_archive_bits() {
    let mut data = [0u8; 32];
    data[0] = 0x85; // In-use, type 5 (File)
    data[1] = 0x00;
    // FileAttributes: read-only + hidden + system + archive
    let attributes = ATTR_READ_ONLY | ATTR_HIDDEN | ATTR_SYSTEM | ATTR_ARCHIVE;
    data[4..6].copy_from_slice(&attributes.to_le_bytes());

    let entry = DirectoryEntry::parse(&data).unwrap();
    let parsed = entry.file_attributes().expect("file attributes");
    assert_eq!(parsed & ATTR_READ_ONLY, ATTR_READ_ONLY);
    assert_eq!(parsed & ATTR_HIDDEN, ATTR_HIDDEN);
    assert_eq!(parsed & ATTR_SYSTEM, ATTR_SYSTEM);
    assert_eq!(parsed & ATTR_ARCHIVE, ATTR_ARCHIVE);
    assert_eq!(parsed & ATTR_DIRECTORY, 0);
}

#[test]
fn parse_directory_entries_preserves_archive_attribute() {
    let mut data = Vec::new();

    let mut file = [0u8; 32];
    file[0] = 0x85;
    file[1] = 0x02;
    file[4..6].copy_from_slice(&(ATTR_READ_ONLY | ATTR_ARCHIVE).to_le_bytes());
    data.extend_from_slice(&file);

    let mut stream = [0u8; 32];
    stream[0] = 0xC0;
    stream[3] = 1;
    stream[20] = 10;
    stream[24] = 4;
    data.extend_from_slice(&stream);

    let mut name = [0u8; 32];
    name[0] = 0xC1;
    name[2] = b'A';
    data.extend_from_slice(&name);

    let entries = parse_directory_entries(&data).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].attributes & ATTR_READ_ONLY, ATTR_READ_ONLY);
    assert_eq!(entries[0].attributes & ATTR_ARCHIVE, ATTR_ARCHIVE);
    assert_eq!(entries[0].attributes & ATTR_HIDDEN, 0);
}

#[test]
fn reject_truncated_secondary_entry_set() {
    let mut data = [0u8; 64];
    data[0] = 0x85;
    data[1] = 2;
    let error = parse_directory_entries(&data).unwrap_err();
    assert!(error.to_string().contains("truncated"));
}

#[test]
fn ignore_entry_set_with_mismatched_name_length() {
    let mut data = Vec::new();
    let mut file = [0u8; 32];
    file[0] = 0x85;
    file[1] = 2;
    data.extend_from_slice(&file);
    let mut stream = [0u8; 32];
    stream[0] = 0xC0;
    stream[3] = 8;
    stream[20] = 2;
    stream[24] = 1;
    data.extend_from_slice(&stream);
    let mut name = [0u8; 32];
    name[0] = 0xC1;
    name[2] = b'A';
    data.extend_from_slice(&name);

    assert!(parse_directory_entries(&data).unwrap().is_empty());
}

#[test]
fn reject_nonzero_entry_set_checksum_mismatch() {
    let mut data = Vec::new();
    let mut file = [0u8; 32];
    file[0] = 0x85;
    file[1] = 2;
    file[2..4].copy_from_slice(&1u16.to_le_bytes());
    data.extend_from_slice(&file);
    let mut stream = [0u8; 32];
    stream[0] = 0xC0;
    stream[3] = 1;
    stream[20] = 2;
    stream[24] = 1;
    data.extend_from_slice(&stream);
    let mut name = [0u8; 32];
    name[0] = 0xC1;
    name[2] = b'A';
    data.extend_from_slice(&name);

    let error = parse_directory_entries(&data).unwrap_err();
    assert!(error.to_string().contains("checksum"));
}

#[test]
fn entry_set_checksum_includes_secondary_checksum_offsets() {
    let mut data = Vec::new();
    let mut file = [0u8; 32];
    file[0] = 0x85;
    file[1] = 2;
    data.extend_from_slice(&file);

    let mut stream = [0u8; 32];
    stream[0] = 0xC0;
    stream[3] = 1;
    stream[20] = 2;
    stream[24] = 1;
    stream[2..4].copy_from_slice(&0x0134u16.to_le_bytes());
    data.extend_from_slice(&stream);

    let mut name = [0u8; 32];
    name[0] = 0xC1;
    name[2] = b'A';
    data.extend_from_slice(&name);

    let checksum = entry_set_checksum(&data);
    data[2..4].copy_from_slice(&checksum.to_le_bytes());
    let entries = parse_directory_entries(&data).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].name, "A");
}
