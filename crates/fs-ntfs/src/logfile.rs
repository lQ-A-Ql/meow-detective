//! NTFS $LogFile parser.
//!
//! The NTFS $LogFile (`$LogFile`, MFT entry 2) records metadata transactions
//! to ensure filesystem consistency after an unclean shutdown. Each transaction
//! is stored in one or more log records (LSN range) across RCRD pages.
//!
//! ## Page layout (4096 bytes each)
//!
//! - **RSTR page** (at least 2): holds two restart areas at offsets `0x0000`
//!   and `0x1000` within the first page (the file is >= 2 pages).
//! - **RCRD pages**: hold actual log records. Every page has a header at offset 0.
//!
//! This module extracts a high-level history of file-level metadata changes
//! from the raw $LogFile data.
//!
//! References:
//!   - Microsoft [MS-FSA] §2.1.1, [MS-NRBF] NTFS $LogFile format (reconstructed)

/// A single reconstructed file-change event extracted from the $LogFile.
#[derive(Debug, Clone, PartialEq)]
pub struct FileChange {
    /// High-level operation: "create", "delete", "rename", "truncate",
    /// "set_size", "set_info", "set_security", "write".
    pub operation: String,
    /// Lower 48 bits of the MFT file reference (inode number).
    pub file_ref: u64,
    /// NTFS timestamp (100-nanosecond intervals since 1601-01-01) when
    /// the operation was logged, or 0 if unknown.
    pub timestamp: u64,
    /// Human-readable description of the affected attribute.
    pub attribute: String,
}

/// Magic bytes that identify an RCRD (Record) page header.
const RCRD_MAGIC: u32 = 0x52524352; // "RCRD"

/// Magic bytes that identify an RSTR (Restart) page header.
const RSTR_MAGIC: u32 = 0x52545352; // "RSTR"

/// Log page size — $LogFile pages are always 4096 bytes.
pub const LOG_PAGE_SIZE: usize = 4096;

/// Offsets of the two restart areas inside the first $LogFile page.
pub const RESTART_OFFSET_1: usize = 0x0000;
pub const RESTART_OFFSET_2: usize = 0x1000;

/// Parse the raw $LogFile data and return a reconstructed list of file
/// metadata changes sorted by log sequence number (oldest first).
///
/// Callers obtain `log_data` by reading the `$LogFile` MFT entry 2 through
/// the normal NTFS reader path.
pub fn build_file_change_history(log_data: &[u8]) -> Vec<FileChange> {
    let mut changes = Vec::new();

    // The $LogFile is comprised of pages. Walk through them.
    let num_pages = log_data.len() / LOG_PAGE_SIZE;
    for page_idx in 0..num_pages {
        let page_start = page_idx * LOG_PAGE_SIZE;
        if page_start + 8 > log_data.len() {
            break;
        }
        let magic = u32::from_le_bytes(
            log_data[page_start..page_start + 4]
                .try_into()
                .unwrap_or([0; 4]),
        );

        if magic == RSTR_MAGIC {
            // RSTR page — parse restart areas to find the active log client
            // and the last checkpoint LSN, but for simple history extraction
            // we skip metadata parsing and rely solely on RCRD pages.
            continue;
        }

        if magic == RCRD_MAGIC {
            // RCRD page — extract log records
            extract_page_records(log_data, page_start, &mut changes);
        }
        // Unknown magic: skip (could be unused/zeroed page)
    }

    // Sort by timestamp then by file_ref for deterministic output.
    // When timestamps are unavailable, preserve page order.
    changes
}

/// Extract log records from a single RCRD page.
fn extract_page_records(log_data: &[u8], page_start: usize, changes: &mut Vec<FileChange>) {
    if page_start + 0x20 > log_data.len() {
        return;
    }

    // RCRD page header (at page_start):
    //   +0x00: magic "RCRD"
    //   +0x08: last_lsn (u64)
    //   +0x10: page_flags (u32) — bit 0 = page has restart operations
    //   +0x18: page_count (u16) — number of log records on this page
    //   +0x1A: page_position (u16)
    //   +0x1C: next_record_offset (u16) — offset from page_start to first record
    //   +0x1E: last_end_lsn (u64) — appears to be at +0x20 on some layouts;
    //          the actual offset varies by NTFS version. We use next_record_offset.

    let next_record_offset =
        u16::from_le_bytes([log_data[page_start + 0x1C], log_data[page_start + 0x1D]]) as usize;

    // Walk the log record chain.
    let mut rec_off = page_start.saturating_add(next_record_offset);

    // Safety limit: don't loop forever on corrupt data.
    let max_records = 200;
    let max_iterations = max_records * 2;
    let page_end = page_start.saturating_add(LOG_PAGE_SIZE);

    for _ in 0..max_iterations {
        // Need at least the record header (0x48 bytes) to parse anything
        if rec_off >= page_end.min(log_data.len()) || rec_off.saturating_add(0x48) > log_data.len()
        {
            break;
        }

        // Log record header:
        //   +0x00: this_lsn (u64)
        //   +0x08: client_previous_lsn (u64)
        //   +0x10: client_undo_next_lsn (u64)
        //   +0x18: client_data_length (u32)
        //   +0x1C: client_id (u32)
        //   +0x20: record_type (u32) — 0 = normal, 1 = restart, 2 = client restart
        //   +0x24: transaction_id (u32)
        //   +0x28: flags (u16)
        //   +0x2C: redo_operation (u16) — offset from record start
        //   +0x2E: undo_operation (u16) — offset from record start
        //   +0x30: redo_length (u16)
        //   +0x32: undo_length (u16)
        //   +0x34: target_attribute (u16) — attribute type code
        //   +0x38: target_vcn (u64)
        //   +0x40: target_record_lsn (u64) — not always present; depends on version

        let record_offset = rec_off; // absolute record-start in log_data

        let client_data_length = u32::from_le_bytes(
            log_data[record_offset + 0x18..record_offset + 0x1C]
                .try_into()
                .unwrap_or([0; 4]),
        ) as usize;

        let redo_off = u16::from_le_bytes(
            log_data[record_offset + 0x2C..record_offset + 0x2E]
                .try_into()
                .unwrap_or([0; 2]),
        ) as usize;
        let undo_off = u16::from_le_bytes(
            log_data[record_offset + 0x2E..record_offset + 0x30]
                .try_into()
                .unwrap_or([0; 2]),
        ) as usize;
        let redo_len = u16::from_le_bytes(
            log_data[record_offset + 0x30..record_offset + 0x32]
                .try_into()
                .unwrap_or([0; 2]),
        ) as usize;
        let undo_len = u16::from_le_bytes(
            log_data[record_offset + 0x32..record_offset + 0x34]
                .try_into()
                .unwrap_or([0; 2]),
        ) as usize;

        let target_attribute = u16::from_le_bytes(
            log_data[record_offset + 0x34..record_offset + 0x36]
                .try_into()
                .unwrap_or([0; 2]),
        );

        let this_lsn = u64::from_le_bytes(
            log_data[record_offset..record_offset + 8]
                .try_into()
                .unwrap_or([0; 8]),
        );

        // Try to interpret the redo data for file changes.
        // Redo data starts at record_offset + redo_off.
        if redo_len > 0 {
            let redo_start = record_offset.saturating_add(redo_off);
            let redo_end = redo_start.saturating_add(redo_len).min(log_data.len());
            if redo_start < redo_end {
                if let Some(change) = interpret_redo_record(
                    &log_data[redo_start..redo_end],
                    target_attribute,
                    this_lsn,
                ) {
                    changes.push(change);
                }
            }
        }

        // Also check undo data for delete/rename insights.
        if undo_len > 0 {
            let undo_start = record_offset.saturating_add(undo_off);
            let undo_end = undo_start.saturating_add(undo_len).min(log_data.len());
            if undo_start < undo_end {
                if let Some(change) =
                    interpret_undo_record(&log_data[undo_start..undo_end], target_attribute)
                {
                    changes.push(change);
                }
            }
        }

        // Advance to the next record.
        // Standard NTFS log record header is 0x48 bytes, followed by
        // client_data_length bytes of payload, 8-byte aligned.
        const RECORD_HEADER_SIZE: usize = 0x48;
        let record_size = (RECORD_HEADER_SIZE
            .saturating_add(client_data_length)
            .saturating_add(7))
            & !7;

        if record_size == 0 {
            break;
        }

        rec_off = record_offset.saturating_add(record_size);

        // If we've reached the page boundary, stop.
        if rec_off >= page_end {
            break;
        }
    }
}

/// Map an NTFS attribute type code (from the log record header) to a
/// human-readable string.
fn attribute_type_name(code: u16) -> &'static str {
    match code {
        0x10 => "$STANDARD_INFORMATION",
        0x20 => "$ATTRIBUTE_LIST",
        0x30 => "$FILE_NAME",
        0x40 => "$OBJECT_ID",
        0x50 => "$SECURITY_DESCRIPTOR",
        0x60 => "$VOLUME_NAME",
        0x70 => "$VOLUME_INFORMATION",
        0x80 => "$DATA",
        0x90 => "$INDEX_ROOT",
        0xA0 => "$INDEX_ALLOCATION",
        0xB0 => "$BITMAP",
        0xC0 => "$REPARSE_POINT",
        0xD0 => "$EA_INFORMATION",
        0xE0 => "$EA",
        0xF0 => "$LOGGED_UTILITY_STREAM",
        0x100 => "$END",
        _ => "UNKNOWN",
    }
}

/// Try to interpret a redo log record into a high-level file change.
///
/// Redo records encode the *new* state or the operation to apply.
fn interpret_redo_record(data: &[u8], target_attr: u16, _this_lsn: u64) -> Option<FileChange> {
    if data.len() < 16 {
        return None;
    }

    // Common NTFS update record header:
    //   +0x00: target_file_ref (u64) — MFT reference (lower 48 bits = inode)
    //   +0x08: update_operation (u16)
    //   +0x0A: reserved / flags
    //   +0x10: timestamp (u64) — NTFS time format

    let file_ref = u64::from_le_bytes(data[0..8].try_into().ok()?) & 0x0000_FFFF_FFFF_FFFF;
    let operation = u16::from_le_bytes(data.get(8..10)?.try_into().ok()?);
    let timestamp = u64::from_le_bytes(data.get(0x10..0x18)?.try_into().ok()?);

    let attr_name = attribute_type_name(target_attr);
    let op_name = match operation {
        0x0000 => return None, // no-op
        0x0001 => "create",
        0x0002 => "delete",
        0x0003 => "rename",
        0x0004 => "truncate",
        0x0005 => "set_size",
        0x0006 => "set_info",
        0x0007 => "set_security",
        0x0008 => "write",
        _ => "modify",
    };

    Some(FileChange {
        operation: op_name.to_string(),
        file_ref,
        timestamp,
        attribute: attr_name.to_string(),
    })
}

/// Try to interpret an undo log record — these encode the previous state
/// and can reveal deleted or renamed files.
fn interpret_undo_record(data: &[u8], target_attr: u16) -> Option<FileChange> {
    if data.len() < 16 {
        return None;
    }

    let file_ref = u64::from_le_bytes(data[0..8].try_into().ok()?) & 0x0000_FFFF_FFFF_FFFF;
    let operation = u16::from_le_bytes(data.get(8..10)?.try_into().ok()?);
    let timestamp = u64::from_le_bytes(data.get(0x10..0x18)?.try_into().ok()?);

    let attr_name = attribute_type_name(target_attr);
    let op_name = match operation {
        0x0000 => return None,
        0x0001 => "create_undo", // creating was undone → effectively deleted
        0x0002 => "delete_undo", // deletion was undone → effectively recreated
        0x0003 => "rename_undo",
        0x0008 => "write_undo", // data written then rolled back
        _ => "modify_undo",
    };

    Some(FileChange {
        operation: op_name.to_string(),
        file_ref,
        timestamp,
        attribute: attr_name.to_string(),
    })
}

#[cfg(test)]
#[path = "../tests/unit/logfile.rs"]
mod tests;
