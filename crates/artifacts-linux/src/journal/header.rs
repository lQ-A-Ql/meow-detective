//! Journal file header parsing.
//!
//! Layout per `journal-def.h` (all integers little-endian):
//!
//! ```text
//! offset  field
//! 0       signature[8] = "LPKSHHRH"
//! 8       compatible_flags (le32)
//! 12      incompatible_flags (le32)
//! 16      state (u8: 0=OFFLINE, 1=ONLINE, 2=ARCHIVED)
//! 24      file_id (sd_id128, 16 bytes; SipHash key for KEYED_HASH files)
//! 88      header_size (le64)
//! 96      arena_size (le64)
//! 152     n_entries (le64)
//! 176     entry_array_offset (le64)
//! ```
//!
//! All fields up to and including `tail_entry_monotonic` (offset 208) exist
//! in every format revision, so 208 is the minimum acceptable header size.

use crate::LinuxArtifactError;

const SIGNATURE: &[u8; 8] = b"LPKSHHRH";
const MIN_HEADER_SIZE: u64 = 208;

pub(super) const INCOMPATIBLE_KEYED_HASH: u32 = 1 << 2;
pub(super) const INCOMPATIBLE_COMPACT: u32 = 1 << 4;
const SUPPORTED_INCOMPATIBLE: u32 = (1 << 0) | (1 << 1) | (1 << 2) | (1 << 3) | (1 << 4);

#[derive(Debug, Clone)]
pub(super) struct Header {
    pub incompatible_flags: u32,
    pub file_id: [u8; 16],
    pub header_size: u64,
    pub arena_size: u64,
    pub entry_array_offset: u64,
}

impl Header {
    pub(super) fn parse(data: &[u8]) -> Result<Self, LinuxArtifactError> {
        if (data.len() as u64) < MIN_HEADER_SIZE {
            return Err(parse_error("data too short to be a systemd journal file"));
        }
        if data[0..8] != *SIGNATURE {
            return Err(parse_error(
                "not a systemd journal file (invalid signature)",
            ));
        }

        let incompatible_flags = read_u32_at(data, 12);
        let unknown = incompatible_flags & !SUPPORTED_INCOMPATIBLE;
        if unknown != 0 {
            return Err(LinuxArtifactError::Unsupported {
                parser: "journal",
                message: format!("unknown incompatible_flags bits 0x{unknown:08x}"),
            });
        }

        let header_size = read_u64_at(data, 88);
        if header_size < MIN_HEADER_SIZE || header_size > data.len() as u64 {
            return Err(parse_error("header_size outside the file bounds"));
        }
        if !header_size.is_multiple_of(8) {
            return Err(parse_error("header_size is not 8-byte aligned"));
        }

        let mut file_id = [0u8; 16];
        file_id.copy_from_slice(&data[24..40]);

        Ok(Self {
            incompatible_flags,
            file_id,
            header_size,
            arena_size: read_u64_at(data, 96),
            entry_array_offset: read_u64_at(data, 176),
        })
    }

    pub(super) fn compact(&self) -> bool {
        self.incompatible_flags & INCOMPATIBLE_COMPACT != 0
    }

    pub(super) fn keyed_hash(&self) -> bool {
        self.incompatible_flags & INCOMPATIBLE_KEYED_HASH != 0
    }

    /// End offset (exclusive) of the object arena. Files still open for
    /// writing (`STATE_ONLINE`) may have been imaged mid-append, so an arena
    /// that extends past the buffer is reported as truncated, not an error.
    pub(super) fn arena_end(&self, data_len: u64) -> (u64, bool) {
        let nominal = self
            .header_size
            .saturating_add(self.arena_size)
            .max(self.header_size);
        if nominal > data_len {
            (data_len, true)
        } else {
            (nominal, false)
        }
    }
}

fn parse_error(message: &str) -> LinuxArtifactError {
    LinuxArtifactError::ParseError {
        parser: "journal",
        message: message.to_string(),
    }
}

fn read_u32_at(data: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(data[offset..offset + 4].try_into().unwrap_or([0; 4]))
}

fn read_u64_at(data: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(data[offset..offset + 8].try_into().unwrap_or([0; 8]))
}
