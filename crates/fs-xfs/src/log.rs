//! XFS log (journal) replay and metadata recovery.
//!
//! The XFS log records filesystem metadata changes in a circular on-disk
//! buffer.  Log records contain operation items (BUF, INODE, EFI/EFD) that
//! describe what changed.  During replay the log is walked from the tail to
//! the head to reconstruct metadata operations that were in progress when
//! the system stopped.  Deleted files whose metadata is still present in
//! the log can be recovered.
//!
//! ## Log layout
//!
//! The log is split into a head block and a tail block.  Records are
//! written sequentially and wrapped.  Each record starts with a
//! `xlog_rec_header_t`.
//!
//! ## Log item classes
//!
//! | Type     | Code | Purpose                                 |
//! |----------|------|-----------------------------------------|
//! | BUF      | 0x1234 | Buffer (block) data                      |
//! | INODE    | 0x1235 | Inode core changes                       |
//! | EFI      | 0x1236 | Extent free intent                       |
//! | EFD      | 0x1237 | Extent free done                         |
//! | QUOTAOFF | 0x1238 | Quota off                                |
//! | BUF_CANCEL| 0x1239 | Buffer cancel (v4+)                      |
//!
//! This module focuses on INODE and BUF items for deleted-file recovery.

use std::io;

use serde::Serialize;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Log header magic: 0xFEED (big-endian).
pub const XLOG_HEADER_MAGIC: u16 = 0xFEED;

/// Log record header size (standard, all versions).
pub const XLOG_REC_HEADER_SIZE: usize = 32;

/// XFS log block size alignment (typically 512 or 4096).
pub const XLOG_DEFAULT_BLOCK_SIZE: u64 = 4096;

/// Log item type codes.
pub const XLOG_ITEM_BUF: u16 = 0x1234;
pub const XLOG_ITEM_INODE: u16 = 0x1235;
pub const XLOG_ITEM_EFI: u16 = 0x1236;
pub const XLOG_ITEM_EFD: u16 = 0x1237;
pub const XLOG_ITEM_QUOTAOFF: u16 = 0x1238;
pub const XLOG_ITEM_BUF_CANCEL: u16 = 0x1239;

/// Log record header offsets.
#[allow(dead_code)]
mod lh_off {
    pub const MAGIC: usize = 0; // u16
    pub const CYCLE: usize = 2; // u16
    pub const VERSION: usize = 4; // u16
    pub const LEN: usize = 6; // u16
    pub const TAIL_LSN_CYCLE: usize = 8; // u16
    pub const TAIL_LSN_BLOCK: usize = 10; // u16
    pub const CRC: usize = 12; // u32
    pub const PREV_CYCLE: usize = 16; // u16
    pub const PREV_BLOCK: usize = 18; // u16
}

// ---------------------------------------------------------------------------
// Data structures
// ---------------------------------------------------------------------------

/// A file recovered from XFS log analysis.
#[derive(Debug, Clone, Serialize)]
pub struct RecoveredFile {
    /// Best-guess original path (may be incomplete if dirent not in log).
    pub original_path: String,
    /// XFS inode number.
    pub inode: u64,
    /// Recovered data blocks from log BUF items.
    pub blocks: Vec<Vec<u8>>,
    /// Total size declared by the recovered inode.
    pub declared_size: u64,
    /// Recovery method description.
    pub recovery_method: String,
    /// Confidence score 0.0–1.0.
    pub confidence: f64,
    /// Number of data blocks recovered.
    pub block_count: u64,
}

/// Parsed XFS log record header.
#[derive(Debug, Clone)]
pub struct LogRecordHeader {
    /// Magic number (0xFEED).
    pub magic: u16,
    /// Log cycle number.
    pub cycle: u16,
    /// Log format version.
    pub version: u16,
    /// Length of the record in bytes (including header).
    pub len: u16,
}

/// A parsed log operation entry.
#[derive(Debug, Clone)]
pub struct XfsLogEntry {
    /// Human-readable operation description.
    pub operation: String,
    /// Inode number targeted (0 if N/A).
    pub target_ino: u64,
    /// Approximate timestamp (from cycle / relative offset).
    pub timestamp: u64,
    /// Raw item data.
    pub data: Vec<u8>,
    /// Item type code.
    pub item_type: u16,
}

/// An inode core extracted from a log INODE_ITEM.
#[derive(Debug, Clone)]
struct LoggedInodeCore {
    ino: u64,
    #[allow(dead_code)]
    mode: u16,
    size: u64,
    /// Number of extents declared when the log entry was written.
    nextents: u32,
    /// di_format at the time of logging.
    format: u8,
    /// Whether this inode appears to be deleted (links_count == 0).
    is_deleted: bool,
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

impl LogRecordHeader {
    /// Parse a log record header from raw bytes.
    pub fn parse(data: &[u8]) -> io::Result<Self> {
        if data.len() < XLOG_REC_HEADER_SIZE {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "log record header too short",
            ));
        }
        let magic = be_u16(data, lh_off::MAGIC);
        if magic != XLOG_HEADER_MAGIC {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "invalid log record magic 0x{:04X}, expected 0x{:04X}",
                    magic, XLOG_HEADER_MAGIC
                ),
            ));
        }
        Ok(Self {
            magic,
            cycle: be_u16(data, lh_off::CYCLE),
            version: be_u16(data, lh_off::VERSION),
            len: be_u16(data, lh_off::LEN),
        })
    }

    /// Full record length in bytes.  When len is 0, bounded at one block.
    pub fn record_len(&self, block_size: u64) -> u64 {
        if self.len == 0 {
            block_size
        } else {
            self.len as u64
        }
    }
}

/// Walk the log data from `start` to `end`, collecting record headers and
/// their payloads.
pub fn collect_log_records(
    log_data: &[u8],
    block_size: u64,
) -> io::Result<Vec<(LogRecordHeader, Vec<u8>)>> {
    let mut records = Vec::new();
    let mut offset = 0usize;

    while offset + XLOG_REC_HEADER_SIZE <= log_data.len() {
        // Check for end-of-log (empty/padding block with zero magic).
        if offset + 2 <= log_data.len() {
            let peek_magic = be_u16(log_data, offset);
            if peek_magic == 0 {
                // Null block — end of log data.
                offset += block_size as usize;
                continue;
            }
        }
        let header = LogRecordHeader::parse(&log_data[offset..])?;
        let rec_len = header.record_len(block_size) as usize;
        let end = (offset + rec_len).min(log_data.len());
        let payload = log_data[offset + XLOG_REC_HEADER_SIZE..end].to_vec();

        if payload.is_empty() {
            // Null record or empty continuation; advance to next block.
            offset += block_size as usize;
            continue;
        }

        records.push((header, payload));
        offset = ((offset + rec_len) as u64).next_multiple_of(block_size) as usize;
    }

    Ok(records)
}

/// Extract log entries from a record payload by scanning for item headers.
///
/// Item headers in XFS log records use a type-code prefix:
///
/// ```text
/// [ op_header: type(u16), len(u16) ]
/// [ item-specific data ...          ]
/// ```
///
/// This is a heuristic scan; a production reader would follow the exact
/// log format described in the XFS on-disk specification.
pub fn parse_log_entries(payload: &[u8]) -> io::Result<Vec<XfsLogEntry>> {
    let mut entries = Vec::new();
    let mut off = 0usize;

    while off + 4 <= payload.len() {
        let item_type = be_u16(payload, off);
        let item_len = be_u16(payload, off + 2) as usize;

        // Sanity: item_len must be at least 4 (header) and within bounds.
        if item_len < 4 || off + item_len > payload.len() {
            // Try advancing by one byte — log formats can be messy.
            off += 2;
            continue;
        }

        let item_data = payload[off + 4..off + item_len].to_vec();

        let (operation, target_ino) = match item_type {
            XLOG_ITEM_INODE => {
                let ino = if item_data.len() >= 8 {
                    be_u64(&item_data, 0)
                } else {
                    0
                };
                ("inode_update".to_string(), ino)
            }
            XLOG_ITEM_BUF => ("buffer_write".to_string(), 0),
            XLOG_ITEM_EFI => ("extent_free_intent".to_string(), 0),
            XLOG_ITEM_EFD => ("extent_free_done".to_string(), 0),
            XLOG_ITEM_QUOTAOFF => ("quota_off".to_string(), 0),
            XLOG_ITEM_BUF_CANCEL => ("buffer_cancel".to_string(), 0),
            _ => {
                // Unknown item type; skip.
                off += 2;
                continue;
            }
        };

        entries.push(XfsLogEntry {
            operation,
            target_ino,
            timestamp: 0, // Timestamp reconstruction is log-cycle dependent.
            data: item_data,
            item_type,
        });

        off += item_len;
    }

    Ok(entries)
}

/// Recover high-level metadata operations from the log.
///
/// Walks the log, parses record headers and item payloads, and returns
/// a flattened list of `XfsLogEntry` records suitable for timeline or
/// artifact ingestion.
pub fn recover_metadata_operations(log_data: &[u8]) -> io::Result<Vec<XfsLogEntry>> {
    let block_size = XLOG_DEFAULT_BLOCK_SIZE;
    let records = collect_log_records(log_data, block_size)?;
    let mut all_entries = Vec::new();

    for (_header, payload) in &records {
        let entries = parse_log_entries(payload)?;
        all_entries.extend(entries);
    }

    Ok(all_entries)
}

/// Recover deleted files from XFS log data.
///
/// Scans log entries for INODE_ITEM records where the inode's link count
/// is zero (indicating deletion) and gathers associated BUF_ITEM data
/// blocks from the same or nearby transactions.
///
/// Returns a list of `RecoveredFile` records.  Path reconstruction is
/// heuristic: XFS directory entries are not guaranteed to be present in
/// the log, so `original_path` uses a synthetic `$OrphanInode{ino}` prefix.
pub fn recover_deleted_inodes(log_data: &[u8]) -> io::Result<Vec<RecoveredFile>> {
    let entries = recover_metadata_operations(log_data)?;
    let mut recovered: Vec<RecoveredFile> = Vec::new();

    // Group entries by inode for INODE_ITEM entries.
    let mut inode_entries: Vec<&XfsLogEntry> = Vec::new();
    let mut buf_data: Vec<Vec<u8>> = Vec::new();

    for entry in &entries {
        match entry.item_type {
            XLOG_ITEM_INODE => inode_entries.push(entry),
            XLOG_ITEM_BUF => buf_data.push(entry.data.clone()),
            _ => {}
        }
    }

    for entry in &inode_entries {
        let ino = entry.target_ino;
        if ino == 0 {
            continue;
        }

        let inode_core = parse_logged_inode_core(&entry.data);
        if let Some(ic) = inode_core {
            if !ic.is_deleted {
                continue;
            }

            let confidence = compute_log_confidence(&ic, buf_data.len() as u64);

            recovered.push(RecoveredFile {
                original_path: format!("$OrphanInode{}/log_recovered_inode_{}", ino, ino),
                inode: ic.ino,
                blocks: buf_data.clone(),
                declared_size: ic.size,
                recovery_method: format!("xlog_inode_item_format_{}", ic.format),
                confidence,
                block_count: buf_data.len() as u64,
            });
        }
    }

    // Also scan BUF items that look like they contain directory-block data.
    for buf in &buf_data {
        if let Some((name, ino)) = extract_dirent_from_buf(buf) {
            // Check if we already have this inode.
            if recovered.iter().any(|r| r.inode == ino) {
                continue;
            }
            // Mark as low-confidence directory-entry hint.
            recovered.push(RecoveredFile {
                original_path: format!("$OrphanInode{}/dirent_hint_{}", ino, name),
                inode: ino,
                blocks: vec![buf.clone()],
                declared_size: buf.len() as u64,
                recovery_method: "xlog_dirent_hint".to_string(),
                confidence: 0.25,
                block_count: 1,
            });
        }
    }

    Ok(recovered)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn be_u16(buf: &[u8], off: usize) -> u16 {
    u16::from_be_bytes([buf[off], buf[off + 1]])
}

fn be_u32(buf: &[u8], off: usize) -> u32 {
    u32::from_be_bytes([buf[off], buf[off + 1], buf[off + 2], buf[off + 3]])
}

fn be_u64(buf: &[u8], off: usize) -> u64 {
    u64::from_be_bytes([
        buf[off],
        buf[off + 1],
        buf[off + 2],
        buf[off + 3],
        buf[off + 4],
        buf[off + 5],
        buf[off + 6],
        buf[off + 7],
    ])
}

/// Parse an inode core from a log INODE_ITEM payload.
///
/// The XFS log records inode cores without the full inode buffer, using
/// the `xfs_log_inode_core` format (approx 176 bytes).  We look for the
/// inode magic `IN` and extract key fields.
fn parse_logged_inode_core(data: &[u8]) -> Option<LoggedInodeCore> {
    // The log inode core starts with a 2-byte magic (0x494E = "IN").
    if data.len() < 2 {
        return None;
    }
    let magic = be_u16(data, 0);
    if magic != 0x494E {
        // Try at offset 4 (log format can have a small prefix header).
        if data.len() >= 6 {
            let magic2 = be_u16(data, 4);
            if magic2 == 0x494E {
                return Some(parse_inode_fields(data, 4));
            }
        }
        // Try at offset 8 (8-byte inode number prefix is common).
        if data.len() >= 10 {
            let magic3 = be_u16(data, 8);
            if magic3 == 0x494E {
                return Some(parse_inode_fields(data, 8));
            }
        }
        return None;
    }
    Some(parse_inode_fields(data, 0))
}

fn parse_inode_fields(data: &[u8], base: usize) -> LoggedInodeCore {
    // Helper: read u16 at offset, or 0 if out of bounds.
    let safe_u16 = |off: usize| -> u16 {
        if off + 2 <= data.len() {
            be_u16(data, off)
        } else {
            0
        }
    };
    // Helper: read u32 at offset, or 0 if out of bounds.
    let safe_u32 = |off: usize| -> u32 {
        if off + 4 <= data.len() {
            be_u32(data, off)
        } else {
            0
        }
    };
    // Helper: read u64 at offset, or 0 if out of bounds.
    let safe_u64 = |off: usize| -> u64 {
        if off + 8 <= data.len() {
            be_u64(data, off)
        } else {
            0
        }
    };
    // Helper: read u8 at offset, or 0 if out of bounds.
    let safe_u8 = |off: usize| -> u8 {
        if off < data.len() {
            data[off]
        } else {
            0
        }
    };

    let _magic = safe_u16(base);
    let mode = safe_u16(base + 2);
    let _version = safe_u8(base + 4);
    let format = safe_u8(base + 5);
    let size = safe_u64(base + 0x38); // di_size
    let nextents = safe_u32(base + 0x4C); // di_nextents
    let _forkoff = safe_u8(base + 0x52);

    // Determine link count: try v3 offset (0x60) first, then v2 offset (0x10).
    let nlink = if base + 0x64 <= data.len() {
        safe_u32(base + 0x60)
    } else if base + 0x12 <= data.len() {
        safe_u16(base + 0x10) as u32
    } else {
        0
    };

    let is_deleted = nlink == 0;

    // Try to extract inode number — may be embedded near the log item.
    // In XFS log format, the inode number often precedes the inode core.
    let ino = if base >= 8 { safe_u64(base - 8) } else { 0 };

    LoggedInodeCore {
        ino,
        mode,
        size,
        nextents,
        format,
        is_deleted,
    }
}

/// Compute a confidence score for log-recovered files.
fn compute_log_confidence(ic: &LoggedInodeCore, num_buf_blocks: u64) -> f64 {
    let mut c: f64 = 0.25; // base: we found a logged inode

    if ic.size > 0 {
        c += 0.15;
    }
    if ic.nextents > 0 {
        c += 0.10;
    }
    if num_buf_blocks > 0 {
        c += 0.25;
        let expected = ic.size.div_ceil(4096);
        if expected > 0 && num_buf_blocks >= expected {
            c += 0.25;
        }
    }

    c.min(1.0)
}

/// Heuristically extract a directory entry name and inode from a raw
/// buffer that might contain directory block data.
///
/// XFS shortform directories store entries inline; v2/block directories
/// store them in the data fork with a header.  This function scans for
/// plausible entry patterns.
fn extract_dirent_from_buf(buf: &[u8]) -> Option<(String, u64)> {
    // Scan for XFS directory entry pattern: namelen(u8) + name + inode(u64).
    let mut off = 0usize;
    while off + 9 < buf.len() {
        let namelen = buf[off] as usize;
        if namelen == 0 || namelen > 255 {
            off += 1;
            continue;
        }
        let name_start = off + 1;
        let name_end = name_start + namelen;
        if name_end + 8 > buf.len() {
            off += 1;
            continue;
        }
        let name = String::from_utf8_lossy(&buf[name_start..name_end]);
        // Reject non-printable names.
        if name.is_empty() || name.chars().any(|c| c.is_control() && c != '\0') {
            off += 1;
            continue;
        }
        let ino = be_u64(buf, name_end);
        if ino > 0 {
            return Some((name.to_string(), ino));
        }
        off = name_end + 8;
    }
    None
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // Fixture builders
    // -----------------------------------------------------------------------

    /// Build a minimal log record header with magic 0xFEED.
    fn build_log_record_header(cycle: u16, version: u16, rec_len: u16) -> Vec<u8> {
        let mut hdr = vec![0u8; XLOG_REC_HEADER_SIZE];
        hdr[lh_off::MAGIC..lh_off::MAGIC + 2].copy_from_slice(&XLOG_HEADER_MAGIC.to_be_bytes());
        hdr[lh_off::CYCLE..lh_off::CYCLE + 2].copy_from_slice(&cycle.to_be_bytes());
        hdr[lh_off::VERSION..lh_off::VERSION + 2].copy_from_slice(&version.to_be_bytes());
        hdr[lh_off::LEN..lh_off::LEN + 2].copy_from_slice(&rec_len.to_be_bytes());
        hdr
    }

    /// Build an INODE_ITEM payload for a deleted inode (nlink=0).
    fn build_inode_item_payload(ino: u64, size: u64, nlink: u32) -> Vec<u8> {
        // The inode number sits before the inode core in a log item.
        let mut data = Vec::new();
        data.extend_from_slice(&ino.to_be_bytes()); // ino at offset 0
        data.resize(8, 0);

        // Mock inode core at offset 8
        let core_off = data.len();
        data.resize(core_off + 104, 0);
        let c = core_off;
        // Magic "IN" at core+0
        data[c..c + 2].copy_from_slice(&0x494Eu16.to_be_bytes());
        // Mode: regular file 0644
        data[c + 2..c + 4].copy_from_slice(&(0x8000u16 | 0o644).to_be_bytes());
        // Version
        data[c + 4] = 2;
        // Format = 2 (EXTENTS)
        data[c + 5] = 2;
        // Size at offset 0x38
        data[c + 0x38..c + 0x40].copy_from_slice(&size.to_be_bytes());
        // nextents at offset 0x4C
        data[c + 0x4C..c + 0x50].copy_from_slice(&1u32.to_be_bytes());
        // nlink at offset 0x60 (v3) or 0x10 (v2)
        if data.len() > c + 0x64 {
            data[c + 0x60..c + 0x64].copy_from_slice(&nlink.to_be_bytes());
        } else {
            data[c + 0x10..c + 0x12].copy_from_slice(&(nlink as u16).to_be_bytes());
        }

        data
    }

    /// Build a BUF_ITEM payload with recognizable content.
    #[allow(dead_code)]
    fn build_buf_item_payload(content: &[u8]) -> Vec<u8> {
        let mut data = vec![0u8; 4 + 512];
        data[0..2].copy_from_slice(&XLOG_ITEM_BUF.to_be_bytes());
        data[2..4].copy_from_slice(&((content.len() as u16) + 4u16).to_be_bytes());
        data[4..4 + content.len()].copy_from_slice(content);
        data
    }

    /// Build a complete log record: header + payload.
    fn build_log_record(items: &[Vec<u8>]) -> Vec<u8> {
        let payload: Vec<u8> = items.iter().flatten().cloned().collect();
        let rec_len = (XLOG_REC_HEADER_SIZE + payload.len()) as u16;
        let mut hdr = build_log_record_header(1, 2, rec_len);
        let mut record = Vec::new();
        record.append(&mut hdr);
        record.extend_from_slice(&payload);
        record
    }

    // -----------------------------------------------------------------------
    // test_parse_log_record_header
    // -----------------------------------------------------------------------

    #[test]
    fn test_parse_log_record_header() {
        let hdr = build_log_record_header(5, 2, 256);
        let parsed = LogRecordHeader::parse(&hdr).unwrap();
        assert_eq!(parsed.magic, XLOG_HEADER_MAGIC);
        assert_eq!(parsed.cycle, 5);
        assert_eq!(parsed.version, 2);
        assert_eq!(parsed.len, 256);
    }

    // -----------------------------------------------------------------------
    // test_log_record_header_invalid_magic
    // -----------------------------------------------------------------------

    #[test]
    fn test_log_record_header_invalid_magic() {
        let mut hdr = build_log_record_header(1, 2, 128);
        hdr[0] = 0xAB;
        hdr[1] = 0xCD;
        let result = LogRecordHeader::parse(&hdr);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("magic"));
    }

    // -----------------------------------------------------------------------
    // test_collect_log_records
    // -----------------------------------------------------------------------

    #[test]
    fn test_collect_log_records() {
        let inode_item = build_inode_item_payload(42, 4096, 0);
        let record = build_log_record(&[inode_item]);

        let mut log_data = record.clone();
        // Pad to block alignment
        log_data.resize(4096, 0);

        let records = collect_log_records(&log_data, 4096).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].0.cycle, 1);
    }

    // -----------------------------------------------------------------------
    // test_parse_inode_item
    // -----------------------------------------------------------------------

    #[test]
    fn test_parse_inode_item() {
        let ino: u64 = 99;
        let size: u64 = 8192;

        // Build a raw payload that looks like: item_type | item_len | inode core
        let mut payload = Vec::new();
        payload.extend_from_slice(&XLOG_ITEM_INODE.to_be_bytes());
        payload.extend_from_slice(&64u16.to_be_bytes()); // item_len
        payload.extend_from_slice(&ino.to_be_bytes());
        payload.resize(12, 0);

        // Inode core at offset 8
        let co = payload.len();
        payload.resize(co + 104, 0);
        payload[co..co + 2].copy_from_slice(&0x494Eu16.to_be_bytes());
        payload[co + 2..co + 4].copy_from_slice(&(0x8000u16 | 0o644).to_be_bytes());
        payload[co + 5] = 2; // format = EXTENTS
        payload[co + 0x38..co + 0x40].copy_from_slice(&size.to_be_bytes());
        payload[co + 0x4C..co + 0x50].copy_from_slice(&2u32.to_be_bytes());
        // nlink = 0 (deleted)
        // In v3 format, nlink is at offset 0x60; ensure buffer is long enough.
        let nlink_off = co + 0x60;
        if nlink_off + 4 <= payload.len() {
            payload[nlink_off..nlink_off + 4].copy_from_slice(&0u32.to_be_bytes());
        }

        let entries = parse_log_entries(&payload).unwrap();
        assert!(!entries.is_empty());
        let inode_entry = entries.iter().find(|e| e.item_type == XLOG_ITEM_INODE);
        assert!(inode_entry.is_some(), "expected an INODE item entry");
    }

    // -----------------------------------------------------------------------
    // test_recover_metadata_operations
    // -----------------------------------------------------------------------

    #[test]
    fn test_recover_metadata_operations() {
        // Build an INODE item payload
        let mut inode_payload = Vec::new();
        inode_payload.extend_from_slice(&XLOG_ITEM_INODE.to_be_bytes());
        inode_payload.extend_from_slice(&64u16.to_be_bytes());
        inode_payload.extend_from_slice(&123u64.to_be_bytes());
        let co = inode_payload.len();
        inode_payload.resize(co + 104, 0);
        inode_payload[co..co + 2].copy_from_slice(&0x494Eu16.to_be_bytes());
        inode_payload[co + 2..co + 4].copy_from_slice(&(0x8000u16 | 0o644).to_be_bytes());
        inode_payload[co + 5] = 2;
        inode_payload[co + 0x38..co + 0x40].copy_from_slice(&4096u64.to_be_bytes());
        inode_payload[co + 0x4C..co + 0x50].copy_from_slice(&1u32.to_be_bytes());
        // nlink=0 at v3 offset
        let nlink_off = co + 0x60;
        if nlink_off + 4 <= inode_payload.len() {
            inode_payload[nlink_off..nlink_off + 4].copy_from_slice(&0u32.to_be_bytes());
        }

        let record = build_log_record(&[inode_payload]);
        let mut log_data = record;
        log_data.resize(4096, 0);

        let entries = recover_metadata_operations(&log_data).unwrap();
        let inode_entries: Vec<_> = entries
            .iter()
            .filter(|e| e.item_type == XLOG_ITEM_INODE)
            .collect();
        assert!(
            !inode_entries.is_empty(),
            "should find at least one INODE item"
        );
    }

    // -----------------------------------------------------------------------
    // test_recover_deleted_inodes
    // -----------------------------------------------------------------------

    #[test]
    fn test_recover_deleted_inodes() {
        // Build a deleted inode item (nlink=0).
        let mut inode_payload = Vec::new();
        inode_payload.extend_from_slice(&XLOG_ITEM_INODE.to_be_bytes());
        inode_payload.extend_from_slice(&64u16.to_be_bytes());
        let ino: u64 = 77;
        inode_payload.extend_from_slice(&ino.to_be_bytes());
        let co = inode_payload.len();
        inode_payload.resize(co + 104, 0);
        inode_payload[co..co + 2].copy_from_slice(&0x494Eu16.to_be_bytes());
        inode_payload[co + 2..co + 4].copy_from_slice(&(0x8000u16 | 0o644).to_be_bytes());
        inode_payload[co + 5] = 2; // format = EXTENTS
        inode_payload[co + 0x38..co + 0x40].copy_from_slice(&1024u64.to_be_bytes());
        inode_payload[co + 0x4C..co + 0x50].copy_from_slice(&1u32.to_be_bytes());
        let nlink_off = co + 0x60;
        if nlink_off + 4 <= inode_payload.len() {
            inode_payload[nlink_off..nlink_off + 4].copy_from_slice(&0u32.to_be_bytes());
        }

        // Also add a BUF item with some data.
        let mut buf_payload = Vec::new();
        buf_payload.extend_from_slice(&XLOG_ITEM_BUF.to_be_bytes());
        buf_payload.extend_from_slice(&20u16.to_be_bytes());
        buf_payload.extend_from_slice(b"recovered content!!");
        buf_payload.resize(24, 0);

        let record = build_log_record(&[inode_payload, buf_payload]);
        let mut log_data = record;
        log_data.resize(8192, 0);

        let recovered = recover_deleted_inodes(&log_data).unwrap();
        // Should find at least the ino=77 entry.
        let found = recovered.iter().any(|r| r.inode == 77);
        assert!(found, "should recover deleted inode 77 from log");
    }

    // -----------------------------------------------------------------------
    // test_extract_dirent_from_buf
    // -----------------------------------------------------------------------

    #[test]
    fn test_extract_dirent_from_buf() {
        let mut buf = vec![0u8; 256];
        // Entry: "readme.txt" -> ino 100
        let namelen = 10u8;
        buf[0] = namelen;
        buf[1..11].copy_from_slice(b"readme.txt");
        buf[11..19].copy_from_slice(&100u64.to_be_bytes());

        let result = extract_dirent_from_buf(&buf);
        assert!(result.is_some());
        let (name, ino) = result.unwrap();
        assert_eq!(name, "readme.txt");
        assert_eq!(ino, 100);
    }

    // -----------------------------------------------------------------------
    // test_extract_dirent_from_buf_no_valid_entry
    // -----------------------------------------------------------------------

    #[test]
    fn test_extract_dirent_from_buf_no_valid_entry() {
        let buf = vec![0u8; 256]; // All zeros
        let result = extract_dirent_from_buf(&buf);
        assert!(result.is_none());
    }

    // -----------------------------------------------------------------------
    // test_empty_log_data
    // -----------------------------------------------------------------------

    #[test]
    fn test_empty_log_data() {
        let records = collect_log_records(&[], 4096).unwrap();
        assert!(records.is_empty());

        let entries = recover_metadata_operations(&[]).unwrap();
        assert!(entries.is_empty());

        let recovered = recover_deleted_inodes(&[]).unwrap();
        assert!(recovered.is_empty());
    }

    // -----------------------------------------------------------------------
    // test_record_len_when_zero
    // -----------------------------------------------------------------------

    #[test]
    fn test_record_len_when_zero() {
        let hdr = build_log_record_header(1, 2, 0); // len=0
        let parsed = LogRecordHeader::parse(&hdr).unwrap();
        assert_eq!(parsed.len, 0);
        assert_eq!(parsed.record_len(4096), 4096); // defaults to block_size
    }

    // -----------------------------------------------------------------------
    // test_parse_logged_inode_core_nlink_nonzero
    // -----------------------------------------------------------------------

    #[test]
    fn test_parse_logged_inode_core_nlink_nonzero() {
        // Build an inode payload with nlink=1 (still linked — not deleted).
        let mut data = Vec::new();
        data.extend_from_slice(&88u64.to_be_bytes()); // ino
        let co = data.len();
        data.resize(co + 104, 0);
        data[co..co + 2].copy_from_slice(&0x494Eu16.to_be_bytes());
        data[co + 2..co + 4].copy_from_slice(&(0x8000u16 | 0o644).to_be_bytes());
        data[co + 5] = 2;
        data[co + 0x38..co + 0x40].copy_from_slice(&512u64.to_be_bytes());
        data[co + 0x4C..co + 0x50].copy_from_slice(&1u32.to_be_bytes());
        let nlink_off = co + 0x60;
        if nlink_off + 4 <= data.len() {
            data[nlink_off..nlink_off + 4].copy_from_slice(&1u32.to_be_bytes());
            // nlink=1
        }

        let core = parse_logged_inode_core(&data);
        assert!(core.is_some());
        assert!(!core.unwrap().is_deleted);
    }
}
