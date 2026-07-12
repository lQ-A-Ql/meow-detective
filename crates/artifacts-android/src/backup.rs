//! Android ADB Backup (.ab) format parser.
//!
//! The ADB backup format consists of a 24-byte header followed by an optional
//! zlib-compressed tar stream.
//!
//! Header layout (24 bytes):
//! - Bytes 0..4: magic "ANDROID BACKUP\n" (14 bytes)
//! - Bytes 14..18: file format version (u32 LE)
//! - Bytes 18..22: flags (u32 LE): bit 0 = compression enable
//! - Bytes 22..24: salt length (u16 LE) if encrypted; 0 otherwise
//!
//! If the flags indicate compression, the payload (after the header) is a
//! zlib stream that decompresses to a tar archive.

use serde::{Deserialize, Serialize};

/// Parsed metadata from an ADB .ab backup file header.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AndroidBackupHeader {
    /// The backup format version (e.g., 1, 2, 3, 4, 5).
    pub version: u32,
    /// Whether the payload is zlib-compressed.
    pub is_compressed: bool,
    /// Whether the backup is encrypted (salt length > 0).
    pub is_encrypted: bool,
    /// Salt length from the header (0 if not encrypted).
    pub salt_length: u16,
}

/// Minimum header size for an ADB backup file.
const HEADER_SIZE: usize = 24;

/// The magic bytes that prefix every ADB backup header.
const MAGIC: &[u8; 14] = b"ANDROID BACKUP";

/// Parse the 24-byte header from an ADB backup .ab file.
///
/// Returns `Ok(Some(header))` if the magic is valid and the header parses
/// correctly. Returns `Ok(None)` if the data does not start with the expected
/// magic (i.e., not an ADB backup file). Returns `Err` if the data is too
/// short to contain a valid header AND contains the magic, or if the magic is
/// not at the start.
pub fn parse_backup_header(data: &[u8]) -> Result<Option<AndroidBackupHeader>, String> {
    if data.is_empty() {
        return Err("ADB backup data is empty".to_string());
    }

    if data.len() < HEADER_SIZE {
        if data.len() >= MAGIC.len() && &data[..MAGIC.len()] == MAGIC {
            return Err(format!(
                "ADB backup header truncated: expected {} bytes, got {}",
                HEADER_SIZE,
                data.len()
            ));
        }
        return Ok(None);
    }

    // Check magic
    if &data[..MAGIC.len()] != MAGIC {
        return Ok(None);
    }

    // Read version at offset 14 (4 bytes, LE)
    let version = u32::from_le_bytes([data[14], data[15], data[16], data[17]]);

    // Read flags at offset 18 (4 bytes, LE)
    let flags = u32::from_le_bytes([data[18], data[19], data[20], data[21]]);

    // Read salt length at offset 22 (2 bytes, LE)
    let salt_length = u16::from_le_bytes([data[22], data[23]]);

    let is_compressed = (flags & 0x1) != 0;
    let is_encrypted = salt_length > 0;

    Ok(Some(AndroidBackupHeader {
        version,
        is_compressed,
        is_encrypted,
        salt_length,
    }))
}

#[cfg(test)]
#[path = "../tests/unit/backup.rs"]
mod tests;
