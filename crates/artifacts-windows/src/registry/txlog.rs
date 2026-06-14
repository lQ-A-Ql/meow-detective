//! Registry transaction log (.LOG1 / .LOG2) parser.
//!
//! Windows registry hives use transaction log files as a write-ahead ring buffer
//! to provide atomicity and durability. **LOG1** is the primary log; **LOG2** is
//! a secondary/dirty log written during crash recovery.
//!
//! ## Format overview
//!
//! * Header page (4096 bytes): magic signature, two sequence numbers, flags.
//! * Entry region: a ring buffer of variable-length transaction records.  When the
//!   buffer fills, the oldest entries are silently overwritten (wraparound).  The
//!   parser detects wraparound by spotting a non-monotonic sequence number and
//!   emits a warning.
//!
//! ## Supported operation types
//!
//! | Code | Operation    | Meaning                                   |
//! |------|-------------|-------------------------------------------|
//! | 0    | CreateKey   | A new sub-key was created.                |
//! | 1    | DeleteKey   | A sub-key (and its values) was deleted.    |
//! | 2    | SetValue    | A value was created or updated.            |
//! | 3    | DeleteValue | A value was deleted from a key.            |
//! | 4    | RenameKey   | A key was renamed.                         |
//!
//! ## References
//!
//! The binary layout is derived from publicly documented forensic analysis of the
//! Windows registry on-disk format.  Real-world .LOG1/.LOG2 files may contain
//! Windows-version-specific extensions; the parser is lenient where possible.

use byteorder::{LittleEndian, ReadBytesExt};
use chrono::{DateTime, TimeZone, Utc};
use std::io::{Cursor, Read, Seek, SeekFrom};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Size of the header page (one 4 KiB sector).
const HEADER_SIZE: u64 = 0x1000;

/// Known magic values for registry transaction logs.
const MAGIC_HVLE: &[u8; 4] = b"HvLE";
const MAGIC_DIRT: &[u8; 4] = b"DIRT";

/// Maximum per-entry size in bytes — guards against runaway allocations on
/// corrupt input.
const MAX_ENTRY_BYTES: u32 = 1_048_576; // 1 MiB

/// Maximum key-path length in UTF-16 code units.
const MAX_KEY_PATH_CHARS: u16 = 32_767;

/// Maximum value-name length in UTF-16 code units.
const MAX_VALUE_NAME_CHARS: u16 = 16_383;

/// The oldest plausible FILETIME (2000-01-01).
const MIN_FILETIME: u64 = 125_911_584_000_000_000;

/// The newest plausible FILETIME (2100-01-01).
const MAX_FILETIME: u64 = 479_666_880_000_000_000;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// A single registry-level mutation recorded in the transaction log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegistryTransactionOperation {
    /// A new sub-key was created.
    CreateKey,
    /// A sub-key and all its children were removed.
    DeleteKey,
    /// A value under a key was created or its data was modified.
    SetValue,
    /// A value was deleted from a key.
    DeleteValue,
    /// A key was renamed.
    RenameKey,
}

/// One transaction-log entry representing a registry write operation.
#[derive(Debug, Clone)]
pub struct RegistryTransaction {
    /// What kind of registry mutation this entry records.
    pub operation: RegistryTransactionOperation,
    /// Full path to the affected registry key (e.g. `"HKLM\\SOFTWARE\\..."`).
    pub key_path: String,
    /// For value operations (`SetValue` / `DeleteValue`), the value name.
    /// `None` for key-only operations (`CreateKey` / `DeleteKey` / `RenameKey`).
    pub value_name: Option<String>,
    /// Previous data blob (present for `SetValue` when an existing value was
    /// overwritten, and for `DeleteValue`).
    pub data_before: Option<Vec<u8>>,
    /// New data blob (present for `CreateKey` / `SetValue` with the new data).
    pub data_after: Option<Vec<u8>>,
    /// Monotonically increasing sequence number assigned by the registry.
    pub sequence_number: u64,
    /// UTC timestamp extracted from the entry header, if it falls within a
    /// plausible range.
    pub timestamp: Option<DateTime<Utc>>,
}

/// Complete result of parsing a single .LOG1 or .LOG2 file.
#[derive(Debug, Clone)]
pub struct TxLogParseResult {
    /// Parsed transactions in order of appearance within the log (may not be
    /// strictly increasing in sequence number after wraparound).
    pub transactions: Vec<RegistryTransaction>,
    /// `true` if this is the primary log (.LOG1); `false` for secondary (.LOG2).
    pub primary: bool,
    /// Non-fatal warnings encountered during parsing (e.g. wraparound detected,
    /// unrecognised entry types, suspicious fields).
    pub warnings: Vec<String>,
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

/// Parse a registry transaction log (.LOG1 or .LOG2) and its companion (if any).
///
/// `data` is the complete contents of the .LOG1 or .LOG2 file.
///
/// # Errors
///
/// Returns `Err(...)` if the header magic is unrecognised or the data is too
/// short to contain even a header.
pub fn parse_transaction_log(data: &[u8]) -> Result<TxLogParseResult, String> {
    if (data.len() as u64) < HEADER_SIZE {
        return Err(format!(
            "transaction log too short: {} bytes (minimum {HEADER_SIZE})",
            data.len()
        ));
    }

    let mut cursor = Cursor::new(data);

    // --- header -----------------------------------------------------------
    let magic = {
        let mut buf = [0u8; 4];
        cursor
            .read_exact(&mut buf)
            .map_err(|e| format!("read magic: {e}"))?;
        buf
    };

    let primary = if &magic == MAGIC_HVLE {
        true
    } else if &magic == MAGIC_DIRT {
        false
    } else {
        return Err(format!(
            "unrecognised transaction-log magic: {:02X?} (expected {:02X?} or {:02X?})",
            magic, MAGIC_HVLE, MAGIC_DIRT
        ));
    };

    let seq1 = cursor.read_u32::<LittleEndian>().unwrap_or(0);
    let seq2 = cursor.read_u32::<LittleEndian>().unwrap_or(0);
    let _flags = cursor.read_u32::<LittleEndian>().unwrap_or(0);

    let _ = (seq1, seq2); // available for caller to cross-check if desired

    // Seek past the rest of the header page to the start of the entry region.
    cursor
        .seek(SeekFrom::Start(HEADER_SIZE))
        .map_err(|e| format!("seek to entry region: {e}"))?;

    // --- entries ----------------------------------------------------------
    let mut transactions: Vec<RegistryTransaction> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();
    let end = data.len() as u64;
    let mut last_seq: Option<u64> = None;
    let mut wraparound_warned = false;

    loop {
        let pos = cursor
            .stream_position()
            .map_err(|e| format!("stream pos: {e}"))?;
        if pos + 4 > end {
            break; // not enough room for even a size field
        }

        let entry_size = match cursor.read_u32::<LittleEndian>() {
            Ok(s) => s,
            Err(_) => break,
        };

        // Zero size == end-of-log sentinel (or padding).
        if entry_size == 0 {
            break;
        }

        // Sanity-check entry size.
        if entry_size < 24 {
            warnings.push(format!(
                "entry at offset {pos:#x} has impossibly small size {entry_size}; stopping"
            ));
            break;
        }

        if entry_size > MAX_ENTRY_BYTES {
            warnings.push(format!(
                "entry at offset {pos:#x} size {entry_size} exceeds maximum {MAX_ENTRY_BYTES}; stopping"
            ));
            break;
        }

        let record_start = pos + 4;
        let record_end = record_start + (entry_size as u64) - 4; // size field already consumed
        if record_end > end {
            // Truncated final entry — the log may have been incompletely written.
            warnings.push(format!(
                "entry at offset {pos:#x} with size {entry_size} extends past EOF; truncating"
            ));
            break;
        }

        // --- read fixed header inside the record --------------------------
        let seq_num = cursor
            .read_u32::<LittleEndian>()
            .map_err(|e| format!("seq_num: {e}"))? as u64;
        let op_raw = cursor
            .read_u16::<LittleEndian>()
            .map_err(|e| format!("op: {e}"))?;
        let _entry_flags = cursor
            .read_u16::<LittleEndian>()
            .map_err(|e| format!("flags: {e}"))?;
        let filetime = cursor
            .read_u64::<LittleEndian>()
            .map_err(|e| format!("timestamp: {e}"))?;

        // Detect wraparound: sequence numbers should be monotonic; a drop
        // signals that the ring buffer wrapped.
        if let Some(prev) = last_seq {
            if seq_num < prev && !wraparound_warned {
                warnings.push(format!(
                    "ring-buffer wraparound detected: sequence {prev} -> {seq_num} (oldest entries were overwritten)"
                ));
                wraparound_warned = true;
            }
        }
        last_seq = Some(seq_num);

        let operation = match op_raw {
            0 => RegistryTransactionOperation::CreateKey,
            1 => RegistryTransactionOperation::DeleteKey,
            2 => RegistryTransactionOperation::SetValue,
            3 => RegistryTransactionOperation::DeleteValue,
            4 => RegistryTransactionOperation::RenameKey,
            other => {
                warnings.push(format!(
                    "entry at offset {pos:#x} has unknown operation type {other}; skipping"
                ));
                cursor
                    .seek(SeekFrom::Start(record_end))
                    .map_err(|e| format!("skip seek: {e}"))?;
                continue;
            }
        };

        let timestamp = if (MIN_FILETIME..=MAX_FILETIME).contains(&filetime) {
            filetime_to_dt(filetime)
        } else {
            None
        };

        // --- key path (UTF-16LE) ------------------------------------------
        let key_path_len = cursor
            .read_u16::<LittleEndian>()
            .map_err(|e| format!("key_path_len: {e}"))?;
        let key_path = if key_path_len == 0 {
            String::new()
        } else {
            if key_path_len > MAX_KEY_PATH_CHARS {
                warnings.push(format!(
                    "entry at offset {pos:#x} key-path len {key_path_len} exceeds limit"
                ));
                cursor
                    .seek(SeekFrom::Start(record_end))
                    .map_err(|e| format!("skip seek: {e}"))?;
                continue;
            }
            read_utf16_string(&mut cursor, key_path_len as usize)
                .map_err(|e| format!("key_path at offset {pos:#x}: {e}"))?
        };

        // --- value name (UTF-16LE) — always consumed from stream ----------
        let parsed_value_name = {
            let val_name_len = cursor
                .read_u16::<LittleEndian>()
                .map_err(|e| format!("value_name_len: {e}"))?;
            if val_name_len == 0 {
                None
            } else {
                if val_name_len > MAX_VALUE_NAME_CHARS {
                    warnings.push(format!(
                        "entry at offset {pos:#x} value-name len {val_name_len} exceeds limit"
                    ));
                    cursor
                        .seek(SeekFrom::Start(record_end))
                        .map_err(|e| format!("skip seek: {e}"))?;
                    continue;
                }
                let name = read_utf16_string(&mut cursor, val_name_len as usize)
                    .map_err(|e| format!("value_name at offset {pos:#x}: {e}"))?;
                Some(name)
            }
        };

        // Only keep value_name for operations that target values.
        let value_name = if matches!(
            operation,
            RegistryTransactionOperation::SetValue | RegistryTransactionOperation::DeleteValue
        ) {
            parsed_value_name
        } else {
            None
        };

        // --- data-before / data-after (variable length blobs) -------------
        let data_before = read_data_blob(&mut cursor)
            .map_err(|e| format!("data_before at offset {pos:#x}: {e}"))?;

        let data_after = read_data_blob(&mut cursor)
            .map_err(|e| format!("data_after at offset {pos:#x}: {e}"))?;

        // Only keep data_before for operations that actually have "before" state.
        let data_before = match operation {
            RegistryTransactionOperation::SetValue | RegistryTransactionOperation::DeleteValue => {
                data_before
            }
            _ => None,
        };

        // Only keep data_after for operations that produce "after" state.
        let data_after = match operation {
            RegistryTransactionOperation::SetValue
            | RegistryTransactionOperation::CreateKey
            | RegistryTransactionOperation::RenameKey => data_after,
            _ => None,
        };

        transactions.push(RegistryTransaction {
            operation,
            key_path,
            value_name,
            data_before,
            data_after,
            sequence_number: seq_num,
            timestamp,
        });
    }

    Ok(TxLogParseResult {
        transactions,
        primary,
        warnings,
    })
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn filetime_to_dt(ft: u64) -> Option<DateTime<Utc>> {
    let secs = (ft / 10_000_000) as i64 - 11_644_473_600;
    Utc.timestamp_opt(secs, ((ft % 10_000_000) * 100) as u32)
        .single()
}

fn read_utf16_string(cursor: &mut Cursor<&[u8]>, code_units: usize) -> Result<String, String> {
    let byte_len = code_units
        .checked_mul(2)
        .ok_or_else(|| "UTF-16 byte length overflow".to_string())?;
    let mut buf = vec![0u8; byte_len];
    cursor
        .read_exact(&mut buf)
        .map_err(|e| format!("read UTF-16 string: {e}"))?;
    let units: Vec<u16> = buf
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    Ok(String::from_utf16_lossy(&units))
}

fn read_data_blob(cursor: &mut Cursor<&[u8]>) -> Result<Option<Vec<u8>>, String> {
    let len = cursor
        .read_u32::<LittleEndian>()
        .map_err(|e| format!("data blob len: {e}"))?;
    if len == 0 {
        return Ok(None);
    }
    if len > MAX_ENTRY_BYTES {
        return Err(format!("data blob length {len} exceeds maximum"));
    }
    let mut buf = vec![0u8; len as usize];
    cursor
        .read_exact(&mut buf)
        .map_err(|e| format!("read data blob: {e}"))?;
    Ok(Some(buf))
}

// ---------------------------------------------------------------------------
// Synthetic fixture builder (for tests)
// ---------------------------------------------------------------------------

#[cfg(test)]
pub(crate) mod fixture {
    use super::*;

    /// Build a synthetic .LOG1 (primary) transaction log with the given entries.
    ///
    /// This is only used in unit tests; it is not exposed in the public API.
    pub fn build_synthetic_log1(entries: &[SyntheticEntry]) -> Vec<u8> {
        build_log_impl(true, entries)
    }

    /// Build a synthetic .LOG2 (secondary) transaction log.
    pub fn build_synthetic_log2(entries: &[SyntheticEntry]) -> Vec<u8> {
        build_log_impl(false, entries)
    }

    /// Simplified description of an entry for test construction.
    #[derive(Debug, Clone)]
    pub struct SyntheticEntry {
        pub operation: u16,
        pub sequence_number: u32,
        pub timestamp: Option<u64>, // FILETIME; None = plausible default
        pub key_path: String,
        pub value_name: Option<String>,
        pub data_before: Option<Vec<u8>>,
        pub data_after: Option<Vec<u8>>,
    }

    fn build_log_impl(primary: bool, entries: &[SyntheticEntry]) -> Vec<u8> {
        // Header page (4096 bytes).
        let mut data = vec![0u8; HEADER_SIZE as usize];

        let magic = if primary { MAGIC_HVLE } else { MAGIC_DIRT };
        data[0..4].copy_from_slice(magic);

        // Sequence bookmarks: seq1 = first entry, seq2 = last entry.
        let seq1 = entries.first().map(|e| e.sequence_number).unwrap_or(0);
        let seq2 = entries.last().map(|e| e.sequence_number).unwrap_or(0);
        data[4..8].copy_from_slice(&seq1.to_le_bytes());
        data[8..12].copy_from_slice(&seq2.to_le_bytes());

        // Flags: bit 0 = dirty (LOG2 always dirty).
        data[12..16].copy_from_slice(&(if primary { 0u32 } else { 1u32 }).to_le_bytes());

        // Append entries after the header.
        for entry in entries {
            let ft = entry.timestamp.unwrap_or(0x01DB_A000_0000_0000); // 2026-07-01-ish

            // Encode key path as UTF-16LE.
            let kp_utf16: Vec<u16> = entry.key_path.encode_utf16().collect();
            let kp_bytes = kp_utf16.len() as u16;

            let vn_utf16: Vec<u16> = entry
                .value_name
                .as_deref()
                .unwrap_or("")
                .encode_utf16()
                .collect();
            let vn_bytes = vn_utf16.len() as u16;

            let db_buf = entry.data_before.clone().unwrap_or_default();
            let da_buf = entry.data_after.clone().unwrap_or_default();

            // Compute entry size.
            let mut entry_bytes = 0u32;
            entry_bytes += 4; // size field itself (not included in total for our format)
            entry_bytes += 4; // seq_num
            entry_bytes += 2; // op_type
            entry_bytes += 2; // flags
            entry_bytes += 8; // timestamp (FILETIME)
            entry_bytes += 2; // key_path_len
            entry_bytes += (kp_utf16.len() as u32) * 2; // key_path data
            entry_bytes += 2; // value_name_len
            entry_bytes += (vn_utf16.len() as u32) * 2; // value_name data
            entry_bytes += 4 + (db_buf.len() as u32); // data_before (len + data)
            entry_bytes += 4 + (da_buf.len() as u32); // data_after  (len + data)

            let start = data.len() as u64;

            // Entry size (the field *includes* itself in the count that the
            // parser reads — but the parser reads size first and treats the
            // rest as record bytes. We write size as total record + the 4
            // bytes we just allocated for the size field, so the parser's
            // `record_end = pos + 4 + (size - 4)` arithmetic works — size
            // is the total including the 4-byte size prefix.)
            let entry_total = entry_bytes;
            data.extend_from_slice(&entry_total.to_le_bytes());
            data.extend_from_slice(&entry.sequence_number.to_le_bytes());
            data.extend_from_slice(&entry.operation.to_le_bytes());
            data.extend_from_slice(&0u16.to_le_bytes()); // flags
            data.extend_from_slice(&ft.to_le_bytes());
            data.extend_from_slice(&kp_bytes.to_le_bytes());
            // Key path data.
            for unit in &kp_utf16 {
                data.extend_from_slice(&unit.to_le_bytes());
            }
            data.extend_from_slice(&vn_bytes.to_le_bytes());
            // Value name data.
            for unit in &vn_utf16 {
                data.extend_from_slice(&unit.to_le_bytes());
            }
            // data_before blob.
            data.extend_from_slice(&(db_buf.len() as u32).to_le_bytes());
            data.extend_from_slice(&db_buf);
            // data_after blob.
            data.extend_from_slice(&(da_buf.len() as u32).to_le_bytes());
            data.extend_from_slice(&da_buf);

            let actual = (data.len() as u64) - start;
            debug_assert_eq!(
                actual, entry_total as u64,
                "entry size mismatch: expected {entry_total}, got {actual}"
            );
        }

        data
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use fixture::*;

    fn plausible_timestamp() -> u64 {
        // 2026-06-14T12:00:00Z approx
        0x01DB_9F8C_0000_0000
    }

    // ------------------------------------------------------------------
    // Header validation
    // ------------------------------------------------------------------

    #[test]
    fn reject_too_short() {
        let err = parse_transaction_log(&[0u8; 100]).unwrap_err();
        assert!(err.contains("too short"));
    }

    #[test]
    fn reject_invalid_magic() {
        let mut data = vec![0u8; 5000];
        data[0..4].copy_from_slice(b"BEEF");
        let err = parse_transaction_log(&data).unwrap_err();
        assert!(err.contains("unrecognised"));
    }

    #[test]
    fn accept_hvle_magic() {
        let mut data = vec![0u8; 5000];
        data[0..4].copy_from_slice(MAGIC_HVLE);
        let result = parse_transaction_log(&data).unwrap();
        assert!(result.primary);
        assert!(result.transactions.is_empty());
    }

    #[test]
    fn accept_dirt_magic() {
        let mut data = vec![0u8; 5000];
        data[0..4].copy_from_slice(MAGIC_DIRT);
        let result = parse_transaction_log(&data).unwrap();
        assert!(!result.primary);
        assert!(result.transactions.is_empty());
    }

    // ------------------------------------------------------------------
    // Entry parsing
    // ------------------------------------------------------------------

    #[test]
    fn parse_single_set_value_entry() {
        let entries = vec![SyntheticEntry {
            operation: 2, // SetValue
            sequence_number: 1,
            timestamp: Some(plausible_timestamp()),
            key_path: "\\Registry\\Machine\\SOFTWARE\\Test".to_string(),
            value_name: Some("KeyName".to_string()),
            data_before: None,
            data_after: Some(b"hello".to_vec()),
        }];
        let data = build_synthetic_log1(&entries);
        let result = parse_transaction_log(&data).unwrap();

        assert!(result.primary);
        assert!(result.warnings.is_empty());
        assert_eq!(result.transactions.len(), 1);

        let txn = &result.transactions[0];
        assert_eq!(txn.operation, RegistryTransactionOperation::SetValue);
        assert_eq!(txn.key_path, "\\Registry\\Machine\\SOFTWARE\\Test");
        assert_eq!(txn.value_name.as_deref(), Some("KeyName"));
        assert_eq!(txn.data_after.as_deref(), Some(b"hello".as_slice()));
        assert!(txn.data_before.is_none());
        assert_eq!(txn.sequence_number, 1);
        assert!(txn.timestamp.is_some());
    }

    #[test]
    fn parse_multiple_entry_types() {
        let entries = vec![
            SyntheticEntry {
                operation: 0, // CreateKey
                sequence_number: 10,
                timestamp: Some(plausible_timestamp()),
                key_path: "\\Registry\\Machine\\SOFTWARE\\NewApp".to_string(),
                value_name: None,
                data_before: None,
                data_after: None,
            },
            SyntheticEntry {
                operation: 2, // SetValue
                sequence_number: 11,
                timestamp: Some(plausible_timestamp()),
                key_path: "\\Registry\\Machine\\SOFTWARE\\NewApp".to_string(),
                value_name: Some("Version".to_string()),
                data_before: None,
                data_after: Some(b"2.0.0".to_vec()),
            },
            SyntheticEntry {
                operation: 3, // DeleteValue
                sequence_number: 12,
                timestamp: Some(plausible_timestamp()),
                key_path: "\\Registry\\Machine\\SOFTWARE\\NewApp".to_string(),
                value_name: Some("TempFlag".to_string()),
                data_before: Some(b"1".to_vec()),
                data_after: None,
            },
            SyntheticEntry {
                operation: 1, // DeleteKey
                sequence_number: 13,
                timestamp: Some(plausible_timestamp()),
                key_path: "\\Registry\\Machine\\SOFTWARE\\NewApp".to_string(),
                value_name: None,
                data_before: None,
                data_after: None,
            },
        ];
        let data = build_synthetic_log1(&entries);
        let result = parse_transaction_log(&data).unwrap();

        assert_eq!(result.transactions.len(), 4);
        assert!(result.warnings.is_empty());

        assert_eq!(
            result.transactions[0].operation,
            RegistryTransactionOperation::CreateKey
        );
        assert_eq!(
            result.transactions[1].operation,
            RegistryTransactionOperation::SetValue
        );
        assert_eq!(
            result.transactions[2].operation,
            RegistryTransactionOperation::DeleteValue
        );
        assert_eq!(
            result.transactions[3].operation,
            RegistryTransactionOperation::DeleteKey
        );
    }

    #[test]
    fn parse_rename_key_entry() {
        let entries = vec![SyntheticEntry {
            operation: 4, // RenameKey
            sequence_number: 100,
            timestamp: Some(plausible_timestamp()),
            key_path: "\\Registry\\Machine\\SOFTWARE\\OldName".to_string(),
            value_name: None,
            data_before: None,
            data_after: Some(b"NewName".to_vec()), // new name in data_after
        }];
        let data = build_synthetic_log1(&entries);
        let result = parse_transaction_log(&data).unwrap();

        assert_eq!(result.transactions.len(), 1);
        assert_eq!(
            result.transactions[0].operation,
            RegistryTransactionOperation::RenameKey
        );
        assert_eq!(
            result.transactions[0].key_path,
            "\\Registry\\Machine\\SOFTWARE\\OldName"
        );
        assert!(result.transactions[0].value_name.is_none());
        assert!(result.transactions[0].data_before.is_none());
        assert_eq!(
            result.transactions[0].data_after.as_deref(),
            Some(b"NewName".as_slice())
        );
    }

    #[test]
    fn parse_value_with_before_and_after() {
        // Simulate an overwrite: old value = "A", new value = "B".
        let entries = vec![SyntheticEntry {
            operation: 2, // SetValue
            sequence_number: 5,
            timestamp: Some(plausible_timestamp()),
            key_path: "\\Registry\\User\\Settings".to_string(),
            value_name: Some("Pref".to_string()),
            data_before: Some(b"A".to_vec()),
            data_after: Some(b"B".to_vec()),
        }];
        let data = build_synthetic_log1(&entries);
        let result = parse_transaction_log(&data).unwrap();

        let txn = &result.transactions[0];
        assert_eq!(txn.data_before.as_deref(), Some(b"A".as_slice()));
        assert_eq!(txn.data_after.as_deref(), Some(b"B".as_slice()));
    }

    // ------------------------------------------------------------------
    // Ring-buffer wraparound
    // ------------------------------------------------------------------

    #[test]
    fn detect_wraparound_non_monotonic_sequence() {
        let entries = vec![
            SyntheticEntry {
                operation: 2,
                sequence_number: 0xFFFF_FFF0,
                timestamp: Some(plausible_timestamp()),
                key_path: "\\Registry\\Machine\\SOFTWARE\\A".to_string(),
                value_name: Some("V".to_string()),
                data_before: None,
                data_after: Some(b"x".to_vec()),
            },
            SyntheticEntry {
                operation: 2,
                sequence_number: 3, // wrapped
                timestamp: Some(plausible_timestamp()),
                key_path: "\\Registry\\Machine\\SOFTWARE\\B".to_string(),
                value_name: Some("V".to_string()),
                data_before: None,
                data_after: Some(b"y".to_vec()),
            },
        ];
        let data = build_synthetic_log1(&entries);
        let result = parse_transaction_log(&data).unwrap();

        assert_eq!(result.transactions.len(), 2);
        assert!(result.warnings.iter().any(|w| w.contains("wraparound")));
    }

    #[test]
    fn monotonic_sequence_no_wraparound_warning() {
        let entries = vec![
            SyntheticEntry {
                operation: 2,
                sequence_number: 1,
                timestamp: Some(plausible_timestamp()),
                key_path: "\\A".to_string(),
                value_name: Some("a".to_string()),
                data_before: None,
                data_after: Some(b"1".to_vec()),
            },
            SyntheticEntry {
                operation: 2,
                sequence_number: 2,
                timestamp: Some(plausible_timestamp()),
                key_path: "\\B".to_string(),
                value_name: Some("b".to_string()),
                data_before: None,
                data_after: Some(b"2".to_vec()),
            },
        ];
        let data = build_synthetic_log1(&entries);
        let result = parse_transaction_log(&data).unwrap();

        assert!(result.warnings.is_empty());
    }

    // ------------------------------------------------------------------
    // Unicode paths
    // ------------------------------------------------------------------

    #[test]
    fn parse_unicode_key_path() {
        let entries = vec![SyntheticEntry {
            operation: 0,
            sequence_number: 1,
            timestamp: Some(plausible_timestamp()),
            key_path: "\\Registry\\Machine\\SOFTWARE\\中文测试".to_string(),
            value_name: None,
            data_before: None,
            data_after: None,
        }];
        let data = build_synthetic_log1(&entries);
        let result = parse_transaction_log(&data).unwrap();

        assert_eq!(result.transactions.len(), 1);
        assert_eq!(
            result.transactions[0].key_path,
            "\\Registry\\Machine\\SOFTWARE\\中文测试"
        );
    }

    // ------------------------------------------------------------------
    // Empty log (header only, no entries)
    // ------------------------------------------------------------------

    #[test]
    fn empty_log_yields_no_transactions() {
        let data = build_synthetic_log1(&[]);
        let result = parse_transaction_log(&data).unwrap();
        assert!(result.transactions.is_empty());
        assert!(result.warnings.is_empty());
    }

    // ------------------------------------------------------------------
    // Zero-size sentinel stops parsing
    // ------------------------------------------------------------------

    #[test]
    fn zero_size_entry_stops_parsing() {
        let entries = vec![SyntheticEntry {
            operation: 2,
            sequence_number: 1,
            timestamp: Some(plausible_timestamp()),
            key_path: "\\A".to_string(),
            value_name: Some("v".to_string()),
            data_before: None,
            data_after: Some(b"x".to_vec()),
        }];
        let mut data = build_synthetic_log1(&entries);
        // Append a zero-size sentinel and then more garbage that should be ignored.
        data.extend_from_slice(&0u32.to_le_bytes());
        // Garbage after sentinel.
        data.extend_from_slice(b"GARBAGE_DATA_SHOULD_NOT_CAUSE_ERRORS");
        let result = parse_transaction_log(&data).unwrap();

        // Should get the single real entry and stop at the zero sentinel.
        assert_eq!(result.transactions.len(), 1);
    }

    // ------------------------------------------------------------------
    // LOG2 (secondary) flag
    // ------------------------------------------------------------------

    #[test]
    fn log2_is_not_primary() {
        let entries = vec![SyntheticEntry {
            operation: 2,
            sequence_number: 1,
            timestamp: Some(plausible_timestamp()),
            key_path: "\\A".to_string(),
            value_name: Some("v".to_string()),
            data_before: None,
            data_after: Some(b"x".to_vec()),
        }];
        let data = build_synthetic_log2(&entries);
        let result = parse_transaction_log(&data).unwrap();
        assert!(!result.primary);
    }

    // ------------------------------------------------------------------
    // Truncated entry at EOF
    // ------------------------------------------------------------------

    #[test]
    fn truncated_entry_warns_and_stops() {
        let entries = vec![SyntheticEntry {
            operation: 2,
            sequence_number: 1,
            timestamp: Some(plausible_timestamp()),
            key_path: "\\A".to_string(),
            value_name: Some("v".to_string()),
            data_before: None,
            data_after: Some(b"x".to_vec()),
        }];
        let mut data = build_synthetic_log1(&entries);
        // Claim a huge entry size past EOF.
        data.extend_from_slice(&0xFFFFu32.to_le_bytes());
        let result = parse_transaction_log(&data).unwrap();
        assert_eq!(result.transactions.len(), 1);
        assert!(result
            .warnings
            .iter()
            .any(|w| w.contains("extends past EOF")));
    }

    // ------------------------------------------------------------------
    // Unknown operation type is skipped
    // ------------------------------------------------------------------

    #[test]
    fn unknown_operation_type_is_skipped() {
        let entries = vec![
            SyntheticEntry {
                operation: 0,
                sequence_number: 1,
                timestamp: Some(plausible_timestamp()),
                key_path: "\\Good".to_string(),
                value_name: None,
                data_before: None,
                data_after: None,
            },
            // Bad entry with invalid op type.
            SyntheticEntry {
                operation: 99,
                sequence_number: 2,
                timestamp: Some(plausible_timestamp()),
                key_path: "\\Bad".to_string(),
                value_name: None,
                data_before: None,
                data_after: None,
            },
            SyntheticEntry {
                operation: 1,
                sequence_number: 3,
                timestamp: Some(plausible_timestamp()),
                key_path: "\\Good2".to_string(),
                value_name: None,
                data_before: None,
                data_after: None,
            },
        ];
        let data = build_synthetic_log1(&entries);
        let result = parse_transaction_log(&data).unwrap();

        // The bad entry should be skipped.
        assert_eq!(result.transactions.len(), 2);
        assert!(result
            .warnings
            .iter()
            .any(|w| w.contains("unknown operation type")));
        assert_eq!(
            result.transactions[0].operation,
            RegistryTransactionOperation::CreateKey
        );
        assert_eq!(
            result.transactions[1].operation,
            RegistryTransactionOperation::DeleteKey
        );
    }

    // ------------------------------------------------------------------
    // Plausible-timestamp filtering
    // ------------------------------------------------------------------

    #[test]
    fn implausible_timestamp_yields_none() {
        let entries = vec![SyntheticEntry {
            operation: 0,
            sequence_number: 1,
            timestamp: Some(0), // year 1601 — implausible
            key_path: "\\Key".to_string(),
            value_name: None,
            data_before: None,
            data_after: None,
        }];
        let data = build_synthetic_log1(&entries);
        let result = parse_transaction_log(&data).unwrap();
        assert!(result.transactions[0].timestamp.is_none());
    }

    // ------------------------------------------------------------------
    // Unicode value names
    // ------------------------------------------------------------------

    #[test]
    fn parse_unicode_value_name() {
        let entries = vec![SyntheticEntry {
            operation: 2,
            sequence_number: 1,
            timestamp: Some(plausible_timestamp()),
            key_path: "\\Key".to_string(),
            value_name: Some("値".to_string()),
            data_before: None,
            data_after: Some(b"data".to_vec()),
        }];
        let data = build_synthetic_log1(&entries);
        let result = parse_transaction_log(&data).unwrap();
        assert_eq!(result.transactions[0].value_name.as_deref(), Some("値"));
    }
}
