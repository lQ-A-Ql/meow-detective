//! exFAT Directory Entry parsing.
//!
//! exFAT uses a set-based directory structure where each file or directory
//! is described by a sequence of directory entries:
//! 1. File Directory Entry (type 5) - metadata
//! 2. Stream Extension (type 0) - data location
//! 3. File Name entries (type 1) - 15 chars each

use crate::types::*;
use chrono::{DateTime, Utc};
use std::io;

/// Parsed directory entry types.
#[derive(Debug, Clone)]
pub enum DirectoryEntry {
    /// File or directory metadata entry.
    File {
        secondary_count: u8,
        file_attributes: u16,
        create_timestamp: Option<DateTime<Utc>>,
        modified_timestamp: Option<DateTime<Utc>>,
        accessed_timestamp: Option<DateTime<Utc>>,
    },
    /// Stream extension with data location info.
    Stream {
        name_length: u8,
        name_hash: u16,
        valid_data_length: u64,
        first_cluster: u32,
        data_length: u64,
        no_fat_chain: bool,
    },
    /// File name fragment (15 UTF-16LE characters).
    FileName { name: String },
    /// Allocation Bitmap.
    Bitmap {
        bitmap_flags: u8,
        first_cluster: u32,
        data_length: u64,
    },
    /// Up-case Table.
    UpcaseTable {
        first_cluster: u32,
        data_length: u64,
    },
    /// Volume Label.
    VolumeLabel { label: String },
    /// Deleted entry (not in use).
    Deleted,
    /// Unknown entry type.
    Unknown(u8),
}

impl DirectoryEntry {
    /// Parse a 32-byte directory entry.
    pub fn parse(data: &[u8]) -> io::Result<Self> {
        if data.len() < DIR_ENTRY_SIZE {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "directory entry too short",
            ));
        }

        let entry_type = data[0];
        let in_use = entry_type & ENTRY_IN_USE != 0;
        // TypeCode is in bits 5-0 (mask out InUse bit 7 and TypeCategory bit 6)
        let type_code = entry_type & 0x3F;

        if !in_use {
            return Ok(Self::Deleted);
        }

        match type_code {
            ENTRY_TYPE_FILE => Self::parse_file_entry(data),
            ENTRY_TYPE_STREAM => Self::parse_stream_entry(data),
            ENTRY_TYPE_FILENAME => Self::parse_filename_entry(data),
            ENTRY_TYPE_UPCASE if data[1] == 0x00 => Self::parse_upcase_entry(data),
            ENTRY_TYPE_LABEL => Self::parse_label_entry(data),
            _ => Ok(Self::Unknown(type_code)),
        }
    }

    fn parse_file_entry(data: &[u8]) -> io::Result<Self> {
        // SAFETY: caller validated data.len() >= DIR_ENTRY_SIZE (32)
        let create_ts =
            u32::from_le_bytes(data[8..12].try_into().map_err(|_| {
                io::Error::new(io::ErrorKind::InvalidData, "invalid create timestamp")
            })?);
        let modified_ts = u32::from_le_bytes(data[12..16].try_into().map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidData, "invalid modified timestamp")
        })?);
        let accessed_ts = u32::from_le_bytes(data[16..20].try_into().map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidData, "invalid accessed timestamp")
        })?);

        Ok(Self::File {
            secondary_count: data[1],
            file_attributes: u16::from_le_bytes([data[4], data[5]]),
            create_timestamp: parse_exfat_timestamp(create_ts, data[20], data[22]),
            modified_timestamp: parse_exfat_timestamp(modified_ts, data[21], data[23]),
            accessed_timestamp: parse_exfat_timestamp(accessed_ts, 0, data[24]),
        })
    }

    fn parse_stream_entry(data: &[u8]) -> io::Result<Self> {
        // SAFETY: caller validated data.len() >= DIR_ENTRY_SIZE (32)
        let valid_data_length = u64::from_le_bytes(data[8..16].try_into().map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidData, "invalid valid data length")
        })?);
        let first_cluster =
            u32::from_le_bytes(data[20..24].try_into().map_err(|_| {
                io::Error::new(io::ErrorKind::InvalidData, "invalid first cluster")
            })?);
        let data_length = u64::from_le_bytes(
            data[24..32]
                .try_into()
                .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid data length"))?,
        );

        Ok(Self::Stream {
            name_length: data[3],
            name_hash: u16::from_le_bytes([data[4], data[5]]),
            valid_data_length,
            first_cluster,
            data_length,
            no_fat_chain: data[1] & NO_FAT_CHAIN != 0,
        })
    }

    fn parse_filename_entry(data: &[u8]) -> io::Result<Self> {
        // FileName is 15 UTF-16LE characters (30 bytes) starting at offset 2
        let chars: Vec<u16> = data[2..32]
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .take_while(|&c| c != 0)
            .collect();

        Ok(Self::FileName {
            name: String::from_utf16_lossy(&chars),
        })
    }

    fn parse_upcase_entry(data: &[u8]) -> io::Result<Self> {
        // SAFETY: caller validated data.len() >= DIR_ENTRY_SIZE (32)
        let first_cluster =
            u32::from_le_bytes(data[20..24].try_into().map_err(|_| {
                io::Error::new(io::ErrorKind::InvalidData, "invalid first cluster")
            })?);
        let data_length = u64::from_le_bytes(
            data[24..32]
                .try_into()
                .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid data length"))?,
        );

        Ok(Self::UpcaseTable {
            first_cluster,
            data_length,
        })
    }

    fn parse_label_entry(data: &[u8]) -> io::Result<Self> {
        let char_count = data[1] as usize;
        let chars: Vec<u16> = data[2..2 + char_count * 2]
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();

        Ok(Self::VolumeLabel {
            label: String::from_utf16_lossy(&chars),
        })
    }

    /// Check if this is a File entry.
    pub fn is_file(&self) -> bool {
        matches!(self, Self::File { .. })
    }

    /// Check if this is a Stream entry.
    pub fn is_stream(&self) -> bool {
        matches!(self, Self::Stream { .. })
    }

    /// Check if this is a FileName entry.
    pub fn is_filename(&self) -> bool {
        matches!(self, Self::FileName { .. })
    }

    /// Check if this entry is deleted.
    pub fn is_deleted(&self) -> bool {
        matches!(self, Self::Deleted)
    }

    /// Get the file attributes if this is a File entry.
    pub fn file_attributes(&self) -> Option<u16> {
        match self {
            Self::File {
                file_attributes, ..
            } => Some(*file_attributes),
            _ => None,
        }
    }

    /// Check if this represents a directory.
    pub fn is_directory(&self) -> bool {
        match self {
            Self::File {
                file_attributes, ..
            } => file_attributes & ATTR_DIRECTORY != 0,
            _ => false,
        }
    }
}

/// A complete file/directory entry set.
#[derive(Debug, Clone)]
pub struct FileEntrySet {
    /// File attributes.
    pub attributes: u16,
    /// Creation time.
    pub created_at: Option<DateTime<Utc>>,
    /// Last modified time.
    pub modified_at: Option<DateTime<Utc>>,
    /// Last accessed time.
    pub accessed_at: Option<DateTime<Utc>>,
    /// First cluster of the data.
    pub first_cluster: u32,
    /// Valid data length (may be less than allocated size).
    pub valid_data_length: u64,
    /// Allocated data length.
    pub data_length: u64,
    /// Whether the data is contiguous (no FAT chain needed).
    pub no_fat_chain: bool,
    /// The file name (concatenated from File Name entries).
    pub name: String,
}

impl FileEntrySet {
    /// Create a new empty FileEntrySet.
    pub fn new() -> Self {
        Self {
            attributes: 0,
            created_at: None,
            modified_at: None,
            accessed_at: None,
            first_cluster: 0,
            valid_data_length: 0,
            data_length: 0,
            no_fat_chain: false,
            name: String::new(),
        }
    }

    /// Check if this is a directory.
    pub fn is_directory(&self) -> bool {
        self.attributes & ATTR_DIRECTORY != 0
    }

    /// Check if this is a file.
    pub fn is_file(&self) -> bool {
        !self.is_directory()
    }
}

impl Default for FileEntrySet {
    fn default() -> Self {
        Self::new()
    }
}

/// Parse a sequence of directory entries into FileEntrySets.
///
/// This function processes the raw directory entries and groups them into
/// complete file entry sets (File + Stream + FileName[]).
pub fn parse_directory_entries(data: &[u8]) -> io::Result<Vec<FileEntrySet>> {
    let mut entries = Vec::new();
    let mut i = 0;

    while i + DIR_ENTRY_SIZE <= data.len() {
        let entry = DirectoryEntry::parse(&data[i..i + DIR_ENTRY_SIZE])?;

        if let DirectoryEntry::File {
            secondary_count,
            file_attributes,
            create_timestamp,
            modified_timestamp,
            accessed_timestamp,
            ..
        } = entry
        {
            let mut file_set = FileEntrySet {
                attributes: file_attributes,
                created_at: create_timestamp,
                modified_at: modified_timestamp,
                accessed_at: accessed_timestamp,
                ..FileEntrySet::new()
            };

            // Parse secondary entries
            let expected_secondaries = secondary_count as usize;
            let mut name_parts = Vec::new();

            for _ in 0..expected_secondaries {
                i += DIR_ENTRY_SIZE;
                if i + DIR_ENTRY_SIZE > data.len() {
                    break;
                }

                let secondary = DirectoryEntry::parse(&data[i..i + DIR_ENTRY_SIZE])?;
                match secondary {
                    DirectoryEntry::Stream {
                        name_length,
                        first_cluster,
                        valid_data_length,
                        data_length,
                        no_fat_chain,
                        ..
                    } => {
                        file_set.first_cluster = first_cluster;
                        file_set.valid_data_length = valid_data_length;
                        file_set.data_length = data_length;
                        file_set.no_fat_chain = no_fat_chain;
                        // Store name_length for validation
                        let _ = name_length;
                    }
                    DirectoryEntry::FileName { name } => {
                        name_parts.push(name);
                    }
                    _ => {} // Skip other secondary types
                }
            }

            file_set.name = name_parts.join("");
            if !file_set.name.is_empty() || file_set.first_cluster >= MIN_CLUSTER {
                entries.push(file_set);
            }
        }

        i += DIR_ENTRY_SIZE;
    }

    Ok(entries)
}

/// Parse an exFAT timestamp.
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
fn parse_exfat_timestamp(
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

    // Create NaiveDateTime
    let naive = chrono::NaiveDate::from_ymd_opt(year, month, day)?.and_hms_milli_opt(
        hour,
        minute,
        second,
        increment_10ms as u32 * 10,
    )?;

    // exFAT UTC offset: signed 15-minute increments
    // 0x00 = UTC, 0xFF = unknown (treat as UTC)
    // 0x01..0xDF = positive offset (+15 to +3435 minutes)
    // 0xE0..0xFE = negative offset (-480 to -15 minutes)
    let offset_minutes: i64 = if utc_offset == 0xFF {
        // Unknown offset — treat as local time (best effort)
        0
    } else if utc_offset <= 0xDF {
        (utc_offset as i64) * 15
    } else {
        ((utc_offset as i64) - 256) * 15
    };

    // Convert local time to UTC by subtracting the offset
    let utc_naive = naive - chrono::Duration::minutes(offset_minutes);
    Some(utc_naive.and_utc())
}

#[cfg(test)]
mod tests {
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
}
