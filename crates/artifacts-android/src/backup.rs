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
mod tests {
    use super::*;

    /// Create a valid test backup header with the correct magic at the start.
    fn make_test_header(version: u32, flags: u32, salt_length: u16) -> Vec<u8> {
        let mut buf = Vec::new();
        // Magic: "ANDROID BACKUP" (14 bytes, including trailing space)
        // A=65 N=78 D=68 R=82 O=79 I=73 D=68 space=32 B=66 A=65 C=67 K=75 U=85 P=80
        buf.extend_from_slice(&[65, 78, 68, 82, 79, 73, 68, 32, 66, 65, 67, 75, 85, 80]);
        // version: 4 bytes LE
        buf.extend_from_slice(&version.to_le_bytes());
        // flags: 4 bytes LE
        buf.extend_from_slice(&flags.to_le_bytes());
        // salt length: 2 bytes LE
        buf.extend_from_slice(&salt_length.to_le_bytes());
        buf
    }

    #[test]
    fn parse_empty_data_returns_error() {
        let result = parse_backup_header(&[]);
        assert!(result.is_err());
    }

    #[test]
    fn parse_non_backup_data_returns_none() {
        let data = b"this is just some random binary data not an adb backup";
        let result = parse_backup_header(data).expect("should not error");
        assert!(result.is_none());
    }

    #[test]
    fn parse_valid_header_uncompressed() {
        let data = make_test_header(3, 0, 0);
        let result = parse_backup_header(&data).expect("should parse");
        assert!(result.is_some());
        let header = result.unwrap();
        assert_eq!(header.version, 3);
        assert!(!header.is_compressed);
        assert!(!header.is_encrypted);
        assert_eq!(header.salt_length, 0);
    }

    #[test]
    fn parse_valid_header_compressed() {
        let data = make_test_header(4, 1, 0);
        let result = parse_backup_header(&data).expect("should parse");
        assert!(result.is_some());
        let header = result.unwrap();
        assert_eq!(header.version, 4);
        assert!(header.is_compressed);
        assert!(!header.is_encrypted);
    }

    #[test]
    fn parse_valid_header_encrypted() {
        let data = make_test_header(5, 0, 32);
        let result = parse_backup_header(&data).expect("should parse");
        assert!(result.is_some());
        let header = result.unwrap();
        assert_eq!(header.version, 5);
        assert!(!header.is_compressed);
        assert!(header.is_encrypted);
        assert_eq!(header.salt_length, 32);
    }

    #[test]
    fn parse_truncated_header_with_magic_returns_error() {
        // Only the magic bytes, no room for version/flags/salt
        let mut data = Vec::new();
        data.extend_from_slice(&[65, 78, 68, 82, 79, 73, 68, 32, 66, 65, 67, 75, 85, 80]);
        let result = parse_backup_header(&data);
        assert!(result.is_err());
    }
}
