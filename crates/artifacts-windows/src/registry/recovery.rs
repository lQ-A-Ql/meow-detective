//! Deleted cell recovery for Windows registry hives.
//!
//! When registry keys/values are deleted, the hive file does not shrink —
//! deleted cells remain as free space (positive cell size) within hbin blocks.
//! Scanning this unallocated space can recover partially-overwritten NK (key)
//! and VK (value) records that were previously deleted.

use chrono::{DateTime, Utc};

use crate::registry::util::filetime_to_dt;
use crate::registry::RegistryError;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Size of the registry base block (always 4 KiB).
const BASE_BLOCK_SIZE: usize = 0x1000;

/// Hbin block magic value.
const HBIN_MAGIC: &[u8; 4] = b"hbin";

/// NK (key node) record signature.
const NK_SIGNATURE: &[u8; 2] = b"nk";

/// VK (value key) record signature.
const VK_SIGNATURE: &[u8; 2] = b"vk";

/// Hbin header size in bytes.
const HBIN_HEADER_SIZE: usize = 32;

/// Maximum key/value name length in bytes to guard against corrupt input.
const MAX_NAME_BYTES: usize = 512;

/// Sentinel value for an invalid cell offset.
const INVALID_OFFSET: u32 = 0xFFFF_FFFF;

/// Minimum NK record body size (from signature through name_len field).
/// 0x50 bytes from cell start = 0x4c bytes after size field.
const MIN_NK_BODY: usize = 0x4c;

/// Minimum VK record body size (from signature through flags field).
/// 0x18 bytes from cell start = 0x14 bytes after size field.
const MIN_VK_BODY: usize = 0x14;

/// The oldest plausible FILETIME (2000-01-01).
const MIN_FILETIME: u64 = 125_911_584_000_000_000;

/// The newest plausible FILETIME (2100-01-01).
const MAX_FILETIME: u64 = 479_666_880_000_000_000;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Represents a hive bin (hbin) block within the registry file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HiveBin {
    /// Absolute byte offset within the hive file data.
    pub offset: usize,
    /// Total size of this bin in bytes.
    pub size: usize,
}

/// A free (unallocated) cell within a hive bin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FreeCell {
    /// Size of the free cell in bytes (including the 4-byte size field).
    pub size: usize,
    /// Absolute byte offset of the cell within the hive file data.
    pub offset: usize,
}

/// A partially recovered deleted registry key (NK record found in free space).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveredKey {
    /// The key name extracted from the NK record (UTF-16LE decoded).
    pub key_name: String,
    /// Last written timestamp, if readable and within plausible range.
    pub last_written: Option<DateTime<Utc>>,
    /// Number of values this key had at the time of deletion.
    pub num_values: u32,
    /// Hive-relative offset of the recovered cell.
    pub cell_offset: u32,
    /// Best-guess parent path based on cell position context.
    pub parent_path_hint: String,
    /// Confidence level: "high" if the record appears intact,
    /// "low" if partially overwritten or corrupted.
    pub confidence: &'static str,
}

/// A partially recovered deleted registry value (VK record found in free space).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveredValue {
    /// The value name extracted from the VK record (UTF-16LE decoded).
    pub value_name: String,
    /// Registry value type (e.g. 1=REG_SZ, 4=REG_DWORD).
    pub value_type: u32,
    /// First 128 bytes of value data as a hex+ASCII preview string.
    pub value_data_preview: String,
    /// Best-guess key path hint (may be empty for orphaned values).
    pub key_path_hint: String,
    /// Confidence level: "high" if the record appears intact,
    /// "low" if partially overwritten or corrupted.
    pub confidence: &'static str,
}

/// Complete result of a deleted registry cell scan.
#[derive(Debug, Clone)]
pub struct RecoverResult {
    /// Recovered deleted key (NK) records.
    pub recovered_keys: Vec<RecoveredKey>,
    /// Recovered deleted value (VK) records.
    pub recovered_values: Vec<RecoveredValue>,
    /// Total number of free cells scanned.
    pub free_cells_scanned: u32,
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Walk all hive bins and collect every free (unallocated) cell.
///
/// Scans the registry hive starting from the first hbin at offset 0x1000.
/// Each hbin is identified by its `"hbin"` magic at a 4096-byte-aligned
/// offset, and cells within each hbin are walked by following the cell-size
/// chain.  Cells with a positive size (free) are collected.
pub fn scan_free_cells(bytes: &[u8]) -> Vec<FreeCell> {
    let mut cells = Vec::new();
    let file_len = bytes.len();
    let mut hbin_offset = BASE_BLOCK_SIZE;

    while hbin_offset + HBIN_HEADER_SIZE <= file_len {
        // Verify hbin magic.
        if bytes.get(hbin_offset..hbin_offset + 4) != Some(HBIN_MAGIC) {
            break;
        }

        let hbin_size = match read_u32(bytes, hbin_offset + 8) {
            Ok(sz) => sz as usize,
            Err(_) => break,
        };

        if hbin_size == 0 || hbin_size % BASE_BLOCK_SIZE != 0 {
            break;
        }

        let hbin_end = hbin_offset.saturating_add(hbin_size).min(file_len);

        // Walk cells within this hbin starting after the header.
        let mut cell_pos = hbin_offset + HBIN_HEADER_SIZE;
        while cell_pos + 4 <= hbin_end {
            let cell_size = match read_i32(bytes, cell_pos) {
                Ok(sz) => sz,
                Err(_) => break,
            };

            if cell_size == 0 {
                break; // end-of-cell-chain sentinel
            }

            if cell_size > 0 {
                cells.push(FreeCell {
                    size: cell_size as usize,
                    offset: cell_pos,
                });
            }

            let step = cell_size.unsigned_abs() as usize;
            if step == 0 {
                break;
            }
            cell_pos = cell_pos.saturating_add(step);
        }

        hbin_offset = hbin_offset.saturating_add(hbin_size);
    }

    cells
}

/// Scan deleted (free) cells in a registry hive to recover deleted keys and values.
///
/// Walks free cells looking for remnant NK and VK record signatures.  Records
/// that appear well-formed are returned with `"high"` confidence; records
/// whose data appears partially overwritten or corrupted are returned with
/// `"low"` confidence.
pub fn scan_deleted_registry_cells(
    bytes: &[u8],
    hive_path: &str,
) -> Result<RecoverResult, RegistryError> {
    // Validate base block magic.
    if bytes.len() < BASE_BLOCK_SIZE {
        return Err(RegistryError::invalid_cell(
            "registry hive too short for base block",
        ));
    }
    if bytes.get(0..4) != Some(b"regf") {
        return Err(RegistryError::invalid_cell(
            "not a valid registry hive (missing 'regf' magic)",
        ));
    }

    let free_cells = scan_free_cells(bytes);
    let free_cells_scanned = free_cells.len() as u32;

    let mut recovered_keys: Vec<RecoveredKey> = Vec::new();
    let mut recovered_values: Vec<RecoveredValue> = Vec::new();

    for cell in &free_cells {
        // The cell data starts after the 4-byte size field.
        let data_start = cell.offset + 4;
        let data_len = cell.size.saturating_sub(4);

        if data_len < 4 {
            continue;
        }

        let sig = bytes.get(data_start..data_start + 2);
        match sig {
            Some(s) if s == NK_SIGNATURE.as_slice() => {
                if let Some(key) = try_recover_nk(bytes, cell, data_start, data_len, hive_path) {
                    recovered_keys.push(key);
                }
            }
            Some(s) if s == VK_SIGNATURE.as_slice() => {
                if let Some(value) = try_recover_vk(bytes, cell, data_start, data_len, hive_path) {
                    recovered_values.push(value);
                }
            }
            _ => {
                // Not a recognizable record — skip.
            }
        }
    }

    Ok(RecoverResult {
        recovered_keys,
        recovered_values,
        free_cells_scanned,
    })
}

// ---------------------------------------------------------------------------
// Internal recovery helpers
// ---------------------------------------------------------------------------

/// Attempt to parse an NK (key node) record from a free cell.
fn try_recover_nk(
    bytes: &[u8],
    cell: &FreeCell,
    data_start: usize,
    data_len: usize,
    hive_path: &str,
) -> Option<RecoveredKey> {
    let _ = hive_path; // reserved for future context-aware hint generation

    if data_len < MIN_NK_BODY {
        return None;
    }

    let cell_data = &bytes[data_start..data_start + data_len.min(512)];

    // Flags at offset 0x02 from data start.
    let flags = cell_data
        .get(2..4)
        .map(|b| u16::from_le_bytes([b[0], b[1]]))
        .unwrap_or(0);
    let name_compressed = flags & 0x20 != 0;

    // Last written at offset 0x04 from data start (8 bytes, FILETIME).
    let last_written_raw = cell_data
        .get(4..12)
        .map(|b| {
            let arr: [u8; 8] = [b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]];
            u64::from_le_bytes(arr)
        })
        .unwrap_or(0);

    let last_written = if (MIN_FILETIME..=MAX_FILETIME).contains(&last_written_raw) {
        filetime_to_dt(last_written_raw)
    } else {
        None
    };

    // Parent cell offset at offset 0x0C from data start.
    let parent_offset = cell_data
        .get(12..16)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .unwrap_or(INVALID_OFFSET);

    // Num values at offset 0x24 from data start (cell offset 0x28).
    let num_values = cell_data
        .get(0x24..0x28)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .unwrap_or(0);

    // Name length at offset 0x48 from data start (cell offset 0x4c).
    let name_len = cell_data
        .get(0x48..0x4a)
        .map(|b| u16::from_le_bytes([b[0], b[1]]))
        .unwrap_or(0) as usize;

    let name_len = name_len.min(MAX_NAME_BYTES);

    // Name at offset 0x4C from data start (cell offset 0x50).
    let name_start = 0x4C;
    let name_bytes = if name_len > 0 && name_start + name_len <= cell_data.len() {
        &cell_data[name_start..name_start + name_len]
    } else {
        &[]
    };

    let key_name = if name_bytes.is_empty() {
        String::new()
    } else if name_compressed {
        // ASCII/Latin-1 compressed name.
        String::from_utf8_lossy(name_bytes).into_owned()
    } else {
        // UTF-16LE name.
        decode_utf16le_lossy(name_bytes)
    };

    // Determine confidence.
    let mut low_confidence = false;

    // Flag 1: free cell is too small for the declared name.
    if name_len > 0 && name_start + name_len > data_len {
        low_confidence = true;
    }

    // Flag 2: name is empty or entirely replacement characters.
    if key_name.is_empty() || key_name.chars().all(|c| c == '\u{FFFD}') {
        low_confidence = true;
    }

    // Flag 3: last_written is zero or out of range.
    if last_written.is_none() && last_written_raw != 0 {
        low_confidence = true;
    }

    // Try to resolve parent path hint.
    let parent_path_hint = resolve_parent_name(bytes, parent_offset);

    let confidence = if low_confidence { "low" } else { "high" };

    // Hive-relative cell offset.
    let cell_offset = (cell.offset.saturating_sub(BASE_BLOCK_SIZE)) as u32;

    Some(RecoveredKey {
        key_name,
        last_written,
        num_values,
        cell_offset,
        parent_path_hint,
        confidence,
    })
}

/// Attempt to parse a VK (value key) record from a free cell.
fn try_recover_vk(
    bytes: &[u8],
    _cell: &FreeCell,
    data_start: usize,
    data_len: usize,
    hive_path: &str,
) -> Option<RecoveredValue> {
    let _ = hive_path; // reserved for future context-aware hint generation

    if data_len < MIN_VK_BODY {
        return None;
    }

    let cell_data = &bytes[data_start..data_start + data_len.min(512)];

    // Name length at offset 0x02 from data start (cell offset 0x06).
    let name_len = cell_data
        .get(2..4)
        .map(|b| u16::from_le_bytes([b[0], b[1]]))
        .unwrap_or(0) as usize;

    let name_len = name_len.min(MAX_NAME_BYTES);

    // Data length at offset 0x04 from data start (cell offset 0x08).
    let data_len_raw = cell_data
        .get(4..8)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .unwrap_or(0);

    // Data offset at offset 0x08 from data start (cell offset 0x0c).
    let data_offset = cell_data
        .get(8..12)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .unwrap_or(INVALID_OFFSET);

    // Value type at offset 0x0C from data start (cell offset 0x10).
    let value_type = cell_data
        .get(12..16)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .unwrap_or(0);

    // Flags at offset 0x10 from data start (cell offset 0x14).
    let flags = cell_data
        .get(16..18)
        .map(|b| u16::from_le_bytes([b[0], b[1]]))
        .unwrap_or(0);
    let name_compressed = flags & 0x01 != 0;

    // Name at offset 0x14 from data start (cell offset 0x18).
    let name_start = 0x14;
    let name_bytes = if name_len > 0 && name_start + name_len <= cell_data.len() {
        &cell_data[name_start..name_start + name_len]
    } else {
        &[]
    };

    let value_name = if name_bytes.is_empty() {
        String::new()
    } else if name_compressed {
        String::from_utf8_lossy(name_bytes).into_owned()
    } else {
        decode_utf16le_lossy(name_bytes)
    };

    // Try to read value data preview.
    let value_data_preview = read_value_data_preview(bytes, data_len_raw, data_offset, data_len);

    // Determine confidence.
    let mut low_confidence = false;

    // Flag 1: name_len exceeds available data.
    if name_len > 0 && name_start + name_len > data_len {
        low_confidence = true;
    }

    // Flag 2: value name is empty or all replacement characters.
    if value_name.is_empty() || value_name.chars().all(|c| c == '\u{FFFD}') {
        low_confidence = true;
    }

    // Flag 3: value type is implausibly large.
    if value_type > 100 {
        low_confidence = true;
    }

    // Flag 4: free cell size is suspiciously small for declared data.
    if data_len_raw & 0x8000_0000 == 0 && (data_len_raw & 0x7FFF_FFFF) > 0 {
        // External data referenced; the free cell itself is only the VK record.
        // That's normal — VK records are small. No penalty.
    }

    let confidence = if low_confidence { "low" } else { "high" };

    let key_path_hint = String::new(); // VK records lack a parent pointer.

    Some(RecoveredValue {
        value_name,
        value_type,
        value_data_preview,
        key_path_hint,
        confidence,
    })
}

/// Try to read the first 128 bytes of value data from inline storage or an
/// external data cell.  Returns a hex+ASCII preview string.
fn read_value_data_preview(
    bytes: &[u8],
    data_len_raw: u32,
    data_offset: u32,
    _vk_data_len: usize,
) -> String {
    let actual_len = data_len_raw & 0x7FFF_FFFF;
    let is_inline = data_len_raw & 0x8000_0000 != 0;

    if actual_len == 0 {
        return "(empty)".to_string();
    }

    let preview_len = (actual_len as usize).min(128);

    // Store inline data in a local binding so it lives long enough.
    let inline_buf = data_offset.to_le_bytes();

    let preview_bytes: &[u8] = if is_inline {
        // Data is stored directly in the data_offset field (up to 4 bytes).
        let len = preview_len.min(4);
        &inline_buf[..len]
    } else {
        // Data is in an external cell.  Try to read from that cell.
        let data_abs = BASE_BLOCK_SIZE.saturating_add(data_offset as usize);
        if data_abs + 8 > bytes.len() {
            return "(data cell out of bounds)".to_string();
        }
        // Check if the data cell is allocated (negative size) or free (positive).
        let cell_size = match read_i32(bytes, data_abs) {
            Ok(sz) => sz,
            Err(_) => return "(data cell unreadable)".to_string(),
        };
        let cell_data_start = data_abs + 4;
        let cell_data_len = cell_size.unsigned_abs() as usize;
        if cell_data_start + preview_len > bytes.len() {
            return "(data cell truncated)".to_string();
        }
        let available = preview_len.min(cell_data_len.saturating_sub(4));
        if available == 0 {
            return "(data cell too small)".to_string();
        }
        &bytes[cell_data_start..cell_data_start + available]
    };

    format_hex_preview(preview_bytes)
}

/// Format bytes as a hex dump with ASCII sidebar.
fn format_hex_preview(data: &[u8]) -> String {
    if data.is_empty() {
        return "(empty)".to_string();
    }

    let mut result = String::with_capacity(data.len() * 4);
    // Hex part.
    for (i, byte) in data.iter().enumerate() {
        if i > 0 {
            result.push(' ');
        }
        let hex = format!("{byte:02X}");
        result.push_str(&hex);
    }
    // Separator.
    result.push_str(" | ");
    // ASCII part.
    for byte in data {
        if byte.is_ascii_graphic() || *byte == b' ' {
            result.push(*byte as char);
        } else {
            result.push('.');
        }
    }
    result
}

/// Resolve a parent key name from a hive-relative cell offset.
///
/// If the parent cell is still allocated and is an NK record, returns its
/// key name.  Otherwise returns a descriptive hint.
fn resolve_parent_name(bytes: &[u8], parent_offset: u32) -> String {
    if parent_offset == INVALID_OFFSET || parent_offset == 0 {
        return "(orphan)".to_string();
    }

    let abs = BASE_BLOCK_SIZE.saturating_add(parent_offset as usize);
    if abs + 8 > bytes.len() {
        return format!("(parent at 0x{parent_offset:X}, out of bounds)");
    }

    // Read cell size.
    let cell_size = match read_i32(bytes, abs) {
        Ok(sz) => sz,
        Err(_) => return format!("(parent at 0x{parent_offset:X}, unreadable)"),
    };

    if cell_size >= 0 {
        // Parent cell is also free (deleted).
        // Try to read its name anyway — it might still be recoverable.
        if cell_size.unsigned_abs() as usize >= 0x50 + 4 {
            let sig = &bytes[abs + 4..abs + 6];
            if sig == NK_SIGNATURE {
                let name = try_read_nk_name_fast(bytes, abs);
                if !name.is_empty() {
                    return format!("(deleted) {name}");
                }
            }
        }
        return format!("(deleted, parent at 0x{parent_offset:X})");
    }

    // Parent is allocated.
    if bytes.get(abs + 4..abs + 6) != Some(NK_SIGNATURE) {
        return format!("(parent at 0x{parent_offset:X}, not an NK cell)");
    }

    let name = try_read_nk_name_fast(bytes, abs);
    if name.is_empty() {
        return format!("(parent at 0x{parent_offset:X})");
    }

    name
}

/// Quickly read an NK record's name at the given absolute cell offset.
fn try_read_nk_name_fast(bytes: &[u8], cell_abs: usize) -> String {
    if cell_abs + 0x50 > bytes.len() {
        return String::new();
    }

    let flags = match cell_abs.checked_add(6) {
        Some(off) if off + 2 <= bytes.len() => u16::from_le_bytes([bytes[off], bytes[off + 1]]),
        _ => return String::new(),
    };
    let compressed = flags & 0x20 != 0;

    let name_len = match cell_abs.checked_add(0x4c) {
        Some(off) if off + 2 <= bytes.len() => {
            u16::from_le_bytes([bytes[off], bytes[off + 1]]) as usize
        }
        _ => return String::new(),
    };

    let name_len = name_len.min(MAX_NAME_BYTES);
    if name_len == 0 {
        return String::new();
    }

    let name_start = cell_abs + 0x50;
    if name_start + name_len > bytes.len() {
        return String::new();
    }

    let name_bytes = &bytes[name_start..name_start + name_len];

    if compressed {
        String::from_utf8_lossy(name_bytes).into_owned()
    } else {
        decode_utf16le_lossy(name_bytes)
    }
}

// ---------------------------------------------------------------------------
// Low-level byte readers
// ---------------------------------------------------------------------------

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, String> {
    let slice = bytes
        .get(offset..offset + 4)
        .ok_or_else(|| format!("u32 at {offset:#x} out of bounds"))?;
    Ok(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

fn read_i32(bytes: &[u8], offset: usize) -> Result<i32, String> {
    let slice = bytes
        .get(offset..offset + 4)
        .ok_or_else(|| format!("i32 at {offset:#x} out of bounds"))?;
    Ok(i32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Decode UTF-16LE bytes to a String, replacing invalid sequences.
fn decode_utf16le_lossy(bytes: &[u8]) -> String {
    if bytes.len() < 2 {
        return String::new();
    }
    let units: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    String::from_utf16_lossy(&units)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ── Synthetic hive builders ───────────────────────────────────────────

    /// Build a minimal valid registry hive with a base block and a single hbin.
    /// `hbin_data` is placed immediately after the hbin header at offset 0x1020.
    fn build_minimal_hive(hbin_data: &[u8]) -> Vec<u8> {
        let hbin_size = (hbin_data.len() + HBIN_HEADER_SIZE).div_ceil(0x1000) * 0x1000;
        let total_size = BASE_BLOCK_SIZE + hbin_size;
        let mut data = vec![0u8; total_size];

        // Base block: "regf" magic.
        data[0..4].copy_from_slice(b"regf");

        // Hbin at 0x1000.
        let hbin_start = BASE_BLOCK_SIZE;
        data[hbin_start..hbin_start + 4].copy_from_slice(HBIN_MAGIC);
        data[hbin_start + 8..hbin_start + 12].copy_from_slice(&(hbin_size as u32).to_le_bytes());

        // Copy caller-provided hbin payload (cells) after hbin header.
        let payload_offset = hbin_start + HBIN_HEADER_SIZE;
        let copy_len = hbin_data.len().min(hbin_size - HBIN_HEADER_SIZE);
        data[payload_offset..payload_offset + copy_len].copy_from_slice(hbin_data);

        data
    }

    /// Encode a string as UTF-16LE bytes.
    fn to_utf16le(s: &str) -> Vec<u8> {
        s.encode_utf16().flat_map(u16::to_le_bytes).collect()
    }

    /// Push exactly `total_bytes` of data for a cell: the 4-byte size field,
    /// followed by `data`, then zero-padded to reach `total_bytes`.
    fn push_cell(payload: &mut Vec<u8>, cell_size: i32, data: &[u8]) {
        let total = cell_size.unsigned_abs() as usize;
        payload.extend_from_slice(&cell_size.to_le_bytes()); // 4 bytes
        payload.extend_from_slice(data);
        let written = 4 + data.len();
        if written < total {
            payload.resize(payload.len() + (total - written), 0);
        }
    }

    /// Build the body of an NK record (without the leading cell size field).
    /// Returns (body, body_len).
    fn nk_body(
        name_utf16: &[u8],
        last_written: u64,
        num_values: u32,
        parent_offset: u32,
    ) -> Vec<u8> {
        let name_len = name_utf16.len();
        let body_size = 0x50 + name_len;
        let mut body = vec![0u8; body_size];
        body[0..2].copy_from_slice(NK_SIGNATURE);
        // flags = 0
        body[4..12].copy_from_slice(&last_written.to_le_bytes());
        // parent at offset 0x0c (cell offset 0x10) — 4 bytes
        body[12..16].copy_from_slice(&parent_offset.to_le_bytes());
        // num_values at cell offset 0x28 → body offset 0x24
        body[0x24..0x28].copy_from_slice(&num_values.to_le_bytes());
        // name_len at cell offset 0x4c → body offset 0x48
        body[0x48..0x4a].copy_from_slice(&(name_len as u16).to_le_bytes());
        // name at cell offset 0x50 → body offset 0x4c
        body[0x4c..0x4c + name_len].copy_from_slice(name_utf16);
        body
    }

    /// Build the body of a VK record (without the leading cell size field).
    fn vk_body(name_utf16: &[u8], value_type: u32, data_len_raw: u32, data_offset: u32) -> Vec<u8> {
        let name_len = name_utf16.len();
        let body_size = 0x14 + name_len;
        let mut body = vec![0u8; body_size];
        body[0..2].copy_from_slice(VK_SIGNATURE);
        // name_len at cell offset 0x06 → body offset 0x02
        body[2..4].copy_from_slice(&(name_len as u16).to_le_bytes());
        // data_len at cell offset 0x08 → body offset 0x04
        body[4..8].copy_from_slice(&data_len_raw.to_le_bytes());
        // data_offset at cell offset 0x0c → body offset 0x08
        body[8..12].copy_from_slice(&data_offset.to_le_bytes());
        // data_type at cell offset 0x10 → body offset 0x0c
        body[12..16].copy_from_slice(&value_type.to_le_bytes());
        // flags at cell offset 0x14 → body offset 0x10 (leave as 0)
        // name at cell offset 0x18 → body offset 0x14
        body[0x14..0x14 + name_len].copy_from_slice(name_utf16);
        body
    }

    fn default_ft() -> u64 {
        0x01DB_A000_0000_0000
    }

    // ── Test: detect_free_cells_in_hbin ───────────────────────────────────

    #[test]
    fn test_detect_free_cells_in_hbin() {
        // Build a hbin with: alloc(-0x40), free(100), alloc(-0x50), free(64).
        let mut payload = Vec::new();

        // Allocated NK cell 1 (0x40 = 64 bytes).
        let a1 = -0x40i32;
        let mut a1data = vec![0u8; 0x40 - 4];
        a1data[0..2].copy_from_slice(NK_SIGNATURE);
        push_cell(&mut payload, a1, &a1data);

        // Free cell 1 (100 bytes).
        push_cell(&mut payload, 100, &[0xDD; 96]);

        // Allocated NK cell 2 (0x50 = 80 bytes).
        let a2 = -0x50i32;
        let mut a2data = vec![0u8; 0x50 - 4];
        a2data[0..2].copy_from_slice(NK_SIGNATURE);
        push_cell(&mut payload, a2, &a2data);

        // Free cell 2 (64 bytes).
        push_cell(&mut payload, 64, &[0xFF; 60]);

        let hive = build_minimal_hive(&payload);

        let cells = scan_free_cells(&hive);
        assert_eq!(cells.len(), 2, "expected 2 free cells");
        assert_eq!(cells[0].size, 100);
        assert_eq!(cells[1].size, 64);

        let hbin_start = BASE_BLOCK_SIZE + HBIN_HEADER_SIZE;
        assert!(cells[0].offset >= hbin_start);
        assert!(cells[1].offset > cells[0].offset);
    }

    // ── Test: recover_deleted_nk_from_free_cell ───────────────────────────

    #[test]
    fn test_recover_deleted_nk_from_free_cell() {
        let key_name = "DeletedKey";
        let name_bytes = to_utf16le(key_name);
        let body = nk_body(&name_bytes, default_ft(), 3, INVALID_OFFSET);
        let cell_size = (body.len() + 4) as i32;

        let mut payload = Vec::new();

        // Leading allocated cell so the cell walk starts correctly.
        let a1 = -0x40i32;
        push_cell(&mut payload, a1, &[0u8; 0x40 - 4]);

        // Compute absolute offset of the free cell before pushing.
        let hbin_payload_base = BASE_BLOCK_SIZE + HBIN_HEADER_SIZE;
        let cell_offset_absolute = hbin_payload_base + payload.len();

        // Free cell containing NK record.
        push_cell(&mut payload, cell_size, &body);

        let hive = build_minimal_hive(&payload);

        let result = scan_deleted_registry_cells(&hive, "test_hive").unwrap();

        assert_eq!(result.free_cells_scanned, 1);
        assert_eq!(result.recovered_keys.len(), 1);
        assert_eq!(result.recovered_values.len(), 0);

        let key = &result.recovered_keys[0];
        assert_eq!(key.key_name, key_name);
        assert_eq!(key.num_values, 3);
        assert!(key.last_written.is_some());
        assert_eq!(key.confidence, "high");

        let expected_hive_relative = cell_offset_absolute - BASE_BLOCK_SIZE;
        assert_eq!(key.cell_offset as usize, expected_hive_relative);
    }

    // ── Test: recover_deleted_vk_from_free_cell ───────────────────────────

    #[test]
    fn test_recover_deleted_vk_from_free_cell() {
        let value_name = "DeletedValue";
        let name_bytes = to_utf16le(value_name);
        // Inline REG_SZ "Hi" → UTF-16LE = 0x0048 0x0069, len=4 bytes
        let inline_data: u32 = 0x00690048; // "H\0i\0" LE
        let body = vk_body(&name_bytes, 1, 4 | 0x8000_0000, inline_data);
        let cell_size = (body.len() + 4) as i32;

        let mut payload = Vec::new();

        // Leading allocated cell.
        let a1 = -0x40i32;
        push_cell(&mut payload, a1, &[0u8; 0x40 - 4]);

        // Free cell containing VK record.
        push_cell(&mut payload, cell_size, &body);

        let hive = build_minimal_hive(&payload);

        let result = scan_deleted_registry_cells(&hive, "test_hive").unwrap();

        assert_eq!(result.free_cells_scanned, 1);
        assert_eq!(result.recovered_keys.len(), 0);
        assert_eq!(result.recovered_values.len(), 1);

        let value = &result.recovered_values[0];
        assert_eq!(value.value_name, value_name);
        assert_eq!(value.value_type, 1); // REG_SZ
        assert!(
            value.value_data_preview.contains("48 00 69 00"),
            "inline 'Hi' in hex, got: {}",
            value.value_data_preview
        );
        assert!(
            value.value_data_preview.contains("H.i."),
            "ASCII sidebar, got: {}",
            value.value_data_preview
        );
        assert_eq!(value.confidence, "high");
    }

    // ── Test: no_free_cells_returns_empty ─────────────────────────────────

    #[test]
    fn test_no_free_cells_returns_empty() {
        let mut payload = Vec::new();

        // Three allocated cells only.
        for _ in 0..3 {
            let alloc = -0x40i32;
            let data = vec![0xAAu8; 0x40 - 4];
            push_cell(&mut payload, alloc, &data);
        }

        let hive = build_minimal_hive(&payload);

        let cells = scan_free_cells(&hive);
        assert!(cells.is_empty());

        let result = scan_deleted_registry_cells(&hive, "test_hive").unwrap();
        assert_eq!(result.free_cells_scanned, 0);
        assert!(result.recovered_keys.is_empty());
        assert!(result.recovered_values.is_empty());
    }

    // ── Test: partially_overwritten_cell_low_confidence ───────────────────

    #[test]
    fn test_partially_overwritten_cell_low_confidence() {
        let key_name = "PartialKey";
        let name_bytes = to_utf16le(key_name);
        let name_len = name_bytes.len();

        // Build a valid NK body but place it in a free cell that is too small
        // for the declared name_len — simulating partial overwrite.
        let actual_body_size = 0x50 + name_len;
        let mut actual_body = vec![0u8; actual_body_size];
        actual_body[0..2].copy_from_slice(NK_SIGNATURE);
        actual_body[4..12].copy_from_slice(&default_ft().to_le_bytes());
        actual_body[0x24..0x28].copy_from_slice(&1u32.to_le_bytes());
        actual_body[0x48..0x4a].copy_from_slice(&(name_len as u16).to_le_bytes());
        actual_body[0x4c..0x4c + name_len].copy_from_slice(&name_bytes);

        // Free cell size: only 0x50+4 = enough for NK header but not the name.
        let truncated_size: i32 = 0x50 + 4;

        let mut payload = Vec::new();

        // Leading allocated cell.
        push_cell(&mut payload, -0x40i32, &[0u8; 0x40 - 4]);

        // Free cell: truncated.
        let mut cell_data = vec![0u8; (truncated_size as usize) - 4];
        let copy = actual_body.len().min(cell_data.len());
        cell_data[..copy].copy_from_slice(&actual_body[..copy]);
        push_cell(&mut payload, truncated_size, &cell_data);

        let hive = build_minimal_hive(&payload);

        let result = scan_deleted_registry_cells(&hive, "test_hive").unwrap();

        assert_eq!(result.free_cells_scanned, 1);
        assert_eq!(result.recovered_keys.len(), 1);
        assert_eq!(result.recovered_keys[0].confidence, "low");
    }

    // ── Test: free_cell_without_signature_is_skipped ──────────────────────

    #[test]
    fn test_free_cell_without_signature_is_skipped() {
        let mut payload = Vec::new();

        // Leading allocated cell.
        push_cell(&mut payload, -0x40i32, &[0u8; 0x40 - 4]);

        // Free cell with garbage data (not NK/VK).
        push_cell(&mut payload, 128, &[0xABu8; 124]);

        let hive = build_minimal_hive(&payload);

        let result = scan_deleted_registry_cells(&hive, "test_hive").unwrap();

        assert_eq!(result.free_cells_scanned, 1);
        assert!(result.recovered_keys.is_empty());
        assert!(result.recovered_values.is_empty());
    }

    // ── Test: invalid_hive_rejected ──────────────────────────────────────

    #[test]
    fn test_invalid_hive_rejected() {
        let data = vec![0u8; 100];
        let err = scan_deleted_registry_cells(&data, "bad").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("too short") || msg.contains("regf"));
    }

    // ── Test: recover_multiple_cells ─────────────────────────────────────

    #[test]
    fn test_recover_multiple_cells() {
        let mut payload = Vec::new();

        // Leading allocated cell.
        push_cell(&mut payload, -0x60i32, &[0u8; 0x60 - 4]);

        // Free NK 1.
        let nk1_body = nk_body(&to_utf16le("KeyAlpha"), default_ft(), 5, INVALID_OFFSET);
        let nk1_size = (nk1_body.len() + 4) as i32;
        push_cell(&mut payload, nk1_size, &nk1_body);

        // Free VK (inline REG_DWORD = 42).
        let vk_body = vk_body(&to_utf16le("ValueGamma"), 4, 4 | 0x8000_0000, 42);
        let vk_size = (vk_body.len() + 4) as i32;
        push_cell(&mut payload, vk_size, &vk_body);

        // Free NK 2.
        let nk2_body = nk_body(&to_utf16le("KeyBeta"), default_ft(), 0, INVALID_OFFSET);
        let nk2_size = (nk2_body.len() + 4) as i32;
        push_cell(&mut payload, nk2_size, &nk2_body);

        let hive = build_minimal_hive(&payload);

        let result = scan_deleted_registry_cells(&hive, "test_hive").unwrap();

        assert_eq!(result.free_cells_scanned, 3);
        assert_eq!(result.recovered_keys.len(), 2);
        assert_eq!(result.recovered_values.len(), 1);

        let key_names: Vec<&str> = result
            .recovered_keys
            .iter()
            .map(|k| k.key_name.as_str())
            .collect();
        assert!(key_names.contains(&"KeyAlpha"));
        assert!(key_names.contains(&"KeyBeta"));

        assert_eq!(result.recovered_values[0].value_name, "ValueGamma");
        assert_eq!(result.recovered_values[0].value_type, 4); // REG_DWORD
    }

    // ── Test: hbin_chain_walking ─────────────────────────────────────────

    #[test]
    fn test_hbin_chain_walking() {
        // Build a hive with two hbins.
        let hbin1_size = 0x2000; // 8 KiB
        let hbin2_size = 0x1000; // 4 KiB
        let total_size = BASE_BLOCK_SIZE + hbin1_size + hbin2_size;
        let mut data = vec![0u8; total_size];

        // Base block.
        data[0..4].copy_from_slice(b"regf");

        // Hbin 1 at 0x1000.
        data[BASE_BLOCK_SIZE..BASE_BLOCK_SIZE + 4].copy_from_slice(HBIN_MAGIC);
        data[BASE_BLOCK_SIZE + 8..BASE_BLOCK_SIZE + 12]
            .copy_from_slice(&(hbin1_size as u32).to_le_bytes());

        // Put one allocated cell followed by a free VK cell.
        let payload1 = BASE_BLOCK_SIZE + HBIN_HEADER_SIZE;
        // Allocated cell (0x40 bytes).
        let alloc = -0x40i32;
        data[payload1..payload1 + 4].copy_from_slice(&alloc.to_le_bytes());
        // Free VK cell at payload1 + 0x40.
        let free1_off = payload1 + 0x40;
        let vk_name = to_utf16le("VK_In_Hbin1");
        let vk_b = vk_body(&vk_name, 4, 4 | 0x8000_0000, 123);
        let vk_total = (vk_b.len() + 4) as i32;
        data[free1_off..free1_off + 4].copy_from_slice(&vk_total.to_le_bytes());
        data[free1_off + 4..free1_off + 4 + vk_b.len()].copy_from_slice(&vk_b);

        // Hbin 2.
        let hbin2_start = BASE_BLOCK_SIZE + hbin1_size;
        data[hbin2_start..hbin2_start + 4].copy_from_slice(HBIN_MAGIC);
        data[hbin2_start + 8..hbin2_start + 12].copy_from_slice(&(hbin2_size as u32).to_le_bytes());

        // Free NK cell at start of hbin 2 payload.
        let free2_off = hbin2_start + HBIN_HEADER_SIZE;
        let nk_name = to_utf16le("NK_In_Hbin2");
        let nk_b = nk_body(&nk_name, default_ft(), 7, INVALID_OFFSET);
        let nk_total = (nk_b.len() + 4) as i32;
        data[free2_off..free2_off + 4].copy_from_slice(&nk_total.to_le_bytes());
        data[free2_off + 4..free2_off + 4 + nk_b.len()].copy_from_slice(&nk_b);

        let result = scan_deleted_registry_cells(&data, "test_hive").unwrap();

        assert_eq!(result.free_cells_scanned, 2);
        assert_eq!(result.recovered_keys.len(), 1);
        assert_eq!(result.recovered_values.len(), 1);
        assert_eq!(result.recovered_keys[0].key_name, "NK_In_Hbin2");
        assert_eq!(result.recovered_values[0].value_name, "VK_In_Hbin1");
    }

    // ── Test: confidence_high_when_intact ─────────────────────────────────

    #[test]
    fn test_confidence_high_when_intact() {
        let mut payload = Vec::new();

        // Leading allocated cell.
        push_cell(&mut payload, -0x40i32, &[0u8; 0x40 - 4]);

        // Free NK with all valid fields.
        let nk_b = nk_body(&to_utf16le("IntactKey"), default_ft(), 2, INVALID_OFFSET);
        let nk_size = (nk_b.len() + 4) as i32;
        push_cell(&mut payload, nk_size, &nk_b);

        let hive = build_minimal_hive(&payload);
        let result = scan_deleted_registry_cells(&hive, "test_hive").unwrap();

        assert_eq!(result.recovered_keys.len(), 1);
        assert_eq!(result.recovered_keys[0].confidence, "high");
        assert_eq!(result.recovered_keys[0].key_name, "IntactKey");
        assert!(result.recovered_keys[0].last_written.is_some());
    }

    // ── Test: parent_path_hint_resolution ─────────────────────────────────

    #[test]
    fn test_parent_path_hint_resolution() {
        let parent_name = "ParentKey";
        let child_name = "ChildKey";

        // Build parent and child NK bodies.
        let parent_body = nk_body(&to_utf16le(parent_name), default_ft(), 0, INVALID_OFFSET);
        let child_body = nk_body(
            &to_utf16le(child_name),
            default_ft(),
            0,
            0x20, // parent_offset = 0x20 (hive-relative, points to parent cell)
        );

        let mut payload = Vec::new();

        // Allocated parent NK cell at hive-relative offset 0x20.
        let parent_size = -((parent_body.len() + 4) as i32);
        push_cell(&mut payload, parent_size, &parent_body);

        // Free child NK cell.
        let child_size = (child_body.len() + 4) as i32;
        push_cell(&mut payload, child_size, &child_body);

        let hive = build_minimal_hive(&payload);

        let result = scan_deleted_registry_cells(&hive, "test_hive").unwrap();

        assert_eq!(result.recovered_keys.len(), 1);
        let key = &result.recovered_keys[0];
        assert_eq!(key.key_name, child_name);
        assert!(
            key.parent_path_hint.contains("ParentKey"),
            "parent_path_hint should contain parent name, got: {}",
            key.parent_path_hint
        );
    }
}
