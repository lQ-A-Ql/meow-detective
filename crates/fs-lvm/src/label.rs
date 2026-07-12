/// LVM2 Physical Volume Label + PV Header parser.
///
/// On-disk layout (sector 1 = offset 512 of the PV):
/// ```text
/// Offset 0-7:    "LABELONE" magic
/// Offset 8-15:    sector number (u64 LE)
/// Offset 16-19:   CRC-32 of bytes 20..512 (u32 LE)
/// Offset 20-23:   data offset to PV header (u32 LE)
/// Offset 24-31:   "LVM2 001" type indicator
/// Offset data_offset+: PV Header (variable)
///   - 32 bytes:    PV UUID (ASCII, no dashes)
///   - 8 bytes:     PV size (u64 LE)
///   - N×16 bytes:  data area descriptors (terminated by all-zeros)
///   - M×16 bytes:  metadata area descriptors (terminated by all-zeros)
/// ```
use std::io::{ErrorKind, Read, Seek, SeekFrom};

use crate::crc;
use crate::error::{LvmError, Result};

// --- Magic constants ---
const LABEL_SCAN_SECTORS: u64 = 4;
const LABEL_SECTOR_SIZE_U64: u64 = 512;
const LABEL_SECTOR_SIZE: usize = 512;
const LABEL_MAGIC: &[u8; 8] = b"LABELONE";
const TYPE_INDICATOR: &[u8; 8] = b"LVM2 001";

// --- Public types ---

/// Parsed physical volume label information.
#[derive(Debug, Clone)]
pub struct LvmLabel {
    /// PV UUID as a 32-character ASCII string (no dashes).
    pub pv_uuid: String,
    /// Total size of the physical volume in bytes.
    pub pv_size: u64,
    /// Data area regions (typically one contiguous region for the actual LV data).
    pub data_areas: Vec<DataRegion>,
    /// Metadata area regions (typically 1–2 copies for redundancy).
    pub metadata_areas: Vec<DataRegion>,
}

/// A contiguous region on the PV, measured from the PV start.
#[derive(Debug, Clone, Copy)]
pub struct DataRegion {
    /// Absolute byte offset from the start of the physical volume.
    pub offset: u64,
    /// Size of the region in bytes.
    pub size: u64,
}

/// Low-level parsed PV label header (before descriptor parsing).
#[derive(Debug)]
struct RawLabel {
    pv_uuid: String,
    pv_size: u64,
    raw_bytes: [u8; LABEL_SECTOR_SIZE],
}

// --- Public API ---

/// Read and parse the LVM2 PV label from the first four sectors of the PV.
///
/// `pv_offset` is the byte offset of the PV start within the reader (typically
/// the partition's LBA start × 512).
pub fn parse_pv_label<R: Read + Seek + ?Sized>(reader: &mut R, pv_offset: u64) -> Result<LvmLabel> {
    let mut first_candidate_error = None;

    for sector_index in 0..LABEL_SCAN_SECTORS {
        let label_offset = pv_offset + sector_index * LABEL_SECTOR_SIZE_U64;
        let mut sector = [0u8; LABEL_SECTOR_SIZE];

        reader.seek(SeekFrom::Start(label_offset))?;
        match reader.read_exact(&mut sector) {
            Ok(()) => {}
            Err(err) if err.kind() == ErrorKind::UnexpectedEof => break,
            Err(err) => return Err(err.into()),
        }

        match parse_label_sector(sector, sector_index) {
            Ok(label) => return Ok(label),
            Err(LvmError::NotLvm) => continue,
            Err(err) => {
                if first_candidate_error.is_none() {
                    first_candidate_error = Some(err);
                }
            }
        }
    }

    Err(first_candidate_error.unwrap_or(LvmError::NotLvm))
}

// --- Internal helpers ---

fn parse_label_sector(sector: [u8; LABEL_SECTOR_SIZE], sector_index: u64) -> Result<LvmLabel> {
    // Validate magic
    if &sector[0..8] != LABEL_MAGIC {
        return Err(LvmError::NotLvm);
    }
    if &sector[24..32] != TYPE_INDICATOR {
        return Err(LvmError::NotLvm);
    }

    let stored_sector = u64::from_le_bytes([
        sector[8], sector[9], sector[10], sector[11], sector[12], sector[13], sector[14],
        sector[15],
    ]);
    if stored_sector != sector_index {
        return Err(LvmError::MetadataParseError {
            line: 0,
            message: format!(
                "label sector header points to sector {} but was found at sector {}",
                stored_sector, sector_index
            ),
        });
    }

    // Verify CRC-32
    if !crc::verify_label_crc(&sector) {
        let stored = u32::from_le_bytes([sector[16], sector[17], sector[18], sector[19]]);
        let computed = crc::lvm_crc32(&sector[20..LABEL_SECTOR_SIZE]);
        return Err(LvmError::LabelCrcMismatch {
            expected: stored,
            actual: computed,
        });
    }

    let data_offset = u32::from_le_bytes([sector[20], sector[21], sector[22], sector[23]]);
    if data_offset as usize + 40 > LABEL_SECTOR_SIZE {
        return Err(LvmError::MetadataParseError {
            line: 0,
            message: format!("data_offset {} exceeds sector boundary", data_offset),
        });
    }

    let raw = RawLabel {
        pv_uuid: parse_pv_uuid(&sector, data_offset),
        pv_size: u64::from_le_bytes([
            sector[data_offset as usize + 32],
            sector[data_offset as usize + 33],
            sector[data_offset as usize + 34],
            sector[data_offset as usize + 35],
            sector[data_offset as usize + 36],
            sector[data_offset as usize + 37],
            sector[data_offset as usize + 38],
            sector[data_offset as usize + 39],
        ]),
        raw_bytes: sector,
    };

    let desc_start = data_offset as usize + 40; // after UUID(32) + size(8)
    let data_areas = parse_descriptors(&raw.raw_bytes, desc_start, "data")?;
    let meta_start = desc_start + data_areas.len() * 16 + 16; // +16 for terminator
    if meta_start >= LABEL_SECTOR_SIZE {
        return Err(LvmError::MetadataParseError {
            line: 0,
            message: format!(
                "metadata descriptor list starts at offset {} outside label sector",
                meta_start
            ),
        });
    }
    let metadata_areas = parse_descriptors(&raw.raw_bytes, meta_start, "metadata")?;

    Ok(LvmLabel {
        pv_uuid: raw.pv_uuid,
        pv_size: raw.pv_size,
        data_areas,
        metadata_areas,
    })
}

fn parse_pv_uuid(sector: &[u8; 512], data_offset: u32) -> String {
    let start = data_offset as usize;
    let end = start + 32;
    if end > 512 {
        return String::new();
    }
    // UUID is ASCII, may be null-terminated or space-padded.
    let bytes = &sector[start..end];
    let len = bytes.iter().position(|&b| b == 0).unwrap_or(32);
    String::from_utf8_lossy(&bytes[..len]).to_string()
}

/// Parse null-terminated descriptor array from the label sector.
fn parse_descriptors(
    sector: &[u8; LABEL_SECTOR_SIZE],
    mut offset: usize,
    list_name: &str,
) -> Result<Vec<DataRegion>> {
    if offset >= sector.len() {
        return Err(LvmError::MetadataParseError {
            line: 0,
            message: format!(
                "{} descriptor list starts at offset {} outside label sector",
                list_name, offset
            ),
        });
    }

    let mut regions = Vec::new();
    loop {
        if offset + 16 > sector.len() {
            return Err(LvmError::MetadataParseError {
                line: 0,
                message: format!(
                    "{} descriptor list is missing an all-zero terminator within label sector",
                    list_name
                ),
            });
        }
        let desc_offset = u64::from_le_bytes([
            sector[offset],
            sector[offset + 1],
            sector[offset + 2],
            sector[offset + 3],
            sector[offset + 4],
            sector[offset + 5],
            sector[offset + 6],
            sector[offset + 7],
        ]);
        let desc_size = u64::from_le_bytes([
            sector[offset + 8],
            sector[offset + 9],
            sector[offset + 10],
            sector[offset + 11],
            sector[offset + 12],
            sector[offset + 13],
            sector[offset + 14],
            sector[offset + 15],
        ]);
        if desc_offset == 0 && desc_size == 0 {
            return Ok(regions); // end-of-list terminator
        }
        // size=0 means "rest of the device" — include it
        regions.push(DataRegion {
            offset: desc_offset,
            size: desc_size,
        });
        offset += 16;
    }
}

// --- Tests ---

#[cfg(test)]
#[path = "../tests/unit/label.rs"]
mod tests;
