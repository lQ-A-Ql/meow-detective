//! exFAT Directory Entry parsing.
//!
//! exFAT uses a set-based directory structure where each file or directory
//! is described by a sequence of directory entries:
//! 1. File Directory Entry (type 5) - metadata
//! 2. Stream Extension (type 0) - data location
//! 3. File Name entries (type 1) - 15 chars each

use crate::time::parse_exfat_date_time;
use crate::types::*;
use chrono::{DateTime, Utc};
use evidence_core::filesystem::{invalid_fs_data, unexpected_fs_eof};
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
        checksum: u32,
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
            return Err(unexpected_fs_eof("directory entry too short"));
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
        let create_date = u16::from_le_bytes([data[8], data[9]]);
        let create_time = u16::from_le_bytes([data[10], data[11]]);
        let modified_date = u16::from_le_bytes([data[12], data[13]]);
        let modified_time = u16::from_le_bytes([data[14], data[15]]);
        let accessed_date = u16::from_le_bytes([data[16], data[17]]);
        let accessed_time = u16::from_le_bytes([data[18], data[19]]);

        Ok(Self::File {
            secondary_count: data[1],
            file_attributes: u16::from_le_bytes([data[4], data[5]]),
            create_timestamp: parse_exfat_date_time(create_date, create_time, data[20], data[22]),
            modified_timestamp: parse_exfat_date_time(
                modified_date,
                modified_time,
                data[21],
                data[23],
            ),
            accessed_timestamp: parse_exfat_date_time(accessed_date, accessed_time, 0, data[24]),
        })
    }

    fn parse_stream_entry(data: &[u8]) -> io::Result<Self> {
        // SAFETY: caller validated data.len() >= DIR_ENTRY_SIZE (32)
        let valid_data_length = u64::from_le_bytes(
            data[8..16]
                .try_into()
                .map_err(|_| invalid_fs_data("invalid valid data length"))?,
        );
        let first_cluster = u32::from_le_bytes(
            data[20..24]
                .try_into()
                .map_err(|_| invalid_fs_data("invalid first cluster"))?,
        );
        let data_length = u64::from_le_bytes(
            data[24..32]
                .try_into()
                .map_err(|_| invalid_fs_data("invalid data length"))?,
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
        let checksum = u32::from_le_bytes(
            data[4..8]
                .try_into()
                .map_err(|_| invalid_fs_data("invalid up-case table checksum"))?,
        );
        let first_cluster = u32::from_le_bytes(
            data[20..24]
                .try_into()
                .map_err(|_| invalid_fs_data("invalid first cluster"))?,
        );
        let data_length = u64::from_le_bytes(
            data[24..32]
                .try_into()
                .map_err(|_| invalid_fs_data("invalid data length"))?,
        );

        Ok(Self::UpcaseTable {
            checksum,
            first_cluster,
            data_length,
        })
    }

    fn parse_label_entry(data: &[u8]) -> io::Result<Self> {
        let char_count = data[1] as usize;
        let byte_len = char_count
            .checked_mul(2)
            .ok_or_else(|| invalid_fs_data("invalid volume label length"))?;
        let end = 2usize
            .checked_add(byte_len)
            .ok_or_else(|| invalid_fs_data("invalid volume label length"))?;
        let label_bytes = data
            .get(2..end)
            .ok_or_else(|| invalid_fs_data("invalid volume label length"))?;
        let chars: Vec<u16> = label_bytes
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
            let set_len = (secondary_count as usize + 1)
                .checked_mul(DIR_ENTRY_SIZE)
                .ok_or_else(|| invalid_fs_data("exFAT directory entry set length overflows"))?;
            let set_end = i
                .checked_add(set_len)
                .ok_or_else(|| invalid_fs_data("exFAT directory entry set offset overflows"))?;
            if set_end > data.len() {
                return Err(unexpected_fs_eof("truncated exFAT directory entry set"));
            }
            let stored_checksum = u16::from_le_bytes([data[i + 2], data[i + 3]]);
            if stored_checksum != 0 && stored_checksum != entry_set_checksum(&data[i..set_end]) {
                return Err(invalid_fs_data(
                    "exFAT directory entry set checksum mismatch",
                ));
            }
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
            let mut entry_name_length: Option<usize> = None;
            let mut has_stream = false;

            for _ in 0..expected_secondaries {
                i += DIR_ENTRY_SIZE;
                if i + DIR_ENTRY_SIZE > data.len() {
                    return Err(unexpected_fs_eof("truncated exFAT directory entry set"));
                }

                let secondary = DirectoryEntry::parse(&data[i..i + DIR_ENTRY_SIZE])?;
                match secondary {
                    DirectoryEntry::Stream {
                        name_length: stream_name_length,
                        first_cluster,
                        valid_data_length,
                        data_length,
                        no_fat_chain,
                        ..
                    } => {
                        if has_stream {
                            return Err(invalid_fs_data(
                                "exFAT directory entry set contains multiple stream extensions",
                            ));
                        }
                        has_stream = true;
                        entry_name_length = Some(stream_name_length as usize);
                        file_set.first_cluster = first_cluster;
                        file_set.valid_data_length = valid_data_length;
                        file_set.data_length = data_length;
                        file_set.no_fat_chain = no_fat_chain;
                    }
                    DirectoryEntry::FileName { name } => {
                        name_parts.push(name);
                    }
                    _ => {} // Skip other secondary types
                }
            }

            file_set.name = name_parts.join("");
            let expected_name_length = entry_name_length.unwrap_or(0);
            let expected_name_entries = expected_name_length.div_ceil(CHARS_PER_FILENAME_ENTRY);
            if has_stream
                && expected_name_length <= MAX_FILENAME_ENTRIES * CHARS_PER_FILENAME_ENTRY
                && expected_name_entries == name_parts.len()
                && file_set.name.encode_utf16().count() == expected_name_length
            {
                entries.push(file_set);
            }
        }

        i += DIR_ENTRY_SIZE;
    }

    Ok(entries)
}

fn entry_set_checksum(data: &[u8]) -> u16 {
    data.iter()
        .enumerate()
        .fold(0u16, |checksum, (index, byte)| {
            // Only the primary File entry's checksum field is excluded. The
            // same offsets in secondary entries are ordinary data bytes.
            if index < DIR_ENTRY_SIZE && (index == 2 || index == 3) {
                checksum
            } else {
                checksum.rotate_right(1).wrapping_add(u16::from(*byte))
            }
        })
}

#[cfg(test)]
#[path = "../tests/unit/dir.rs"]
mod tests;
