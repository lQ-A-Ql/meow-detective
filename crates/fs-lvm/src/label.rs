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
use std::io::{Read, Seek, SeekFrom};

use crate::crc;
use crate::error::{LvmError, Result};

// --- Magic constants ---
const LABEL_SECTOR_OFFSET: u64 = 512;
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

/// Read and parse the LVM2 PV label from sector 1 of the PV.
///
/// `pv_offset` is the byte offset of the PV start within the reader (typically
/// the partition's LBA start × 512).
pub fn parse_pv_label<R: Read + Seek>(reader: &mut R, pv_offset: u64) -> Result<LvmLabel> {
    let label_offset = pv_offset + LABEL_SECTOR_OFFSET;
    let mut sector = [0u8; LABEL_SECTOR_SIZE];

    reader.seek(SeekFrom::Start(label_offset))?;
    reader.read_exact(&mut sector)?;

    // Validate magic
    if &sector[0..8] != LABEL_MAGIC {
        return Err(LvmError::NotLvm);
    }
    if &sector[24..32] != TYPE_INDICATOR {
        return Err(LvmError::NotLvm);
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
    let data_areas = parse_descriptors(&raw.raw_bytes, desc_start);
    let meta_start = desc_start + data_areas.len() * 16 + 16; // +16 for terminator
    let metadata_areas = parse_descriptors(&raw.raw_bytes, meta_start.min(LABEL_SECTOR_SIZE));

    Ok(LvmLabel {
        pv_uuid: raw.pv_uuid,
        pv_size: raw.pv_size,
        data_areas,
        metadata_areas,
    })
}

// --- Internal helpers ---

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
fn parse_descriptors(sector: &[u8; 512], mut offset: usize) -> Vec<DataRegion> {
    let mut regions = Vec::new();
    loop {
        if offset + 16 > sector.len() {
            break;
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
            break; // end-of-list terminator
        }
        // size=0 means "rest of the device" — include it
        regions.push(DataRegion {
            offset: desc_offset,
            size: desc_size,
        });
        offset += 16;
    }
    regions
}

// --- Tests ---

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    /// Build a minimal disk image with sector 0 (empty) + sector 1 (PV label).
    fn build_label_disk(pv_uuid: &str, pv_size: u64) -> Vec<u8> {
        let mut disk = vec![0u8; 1024]; // sector 0 empty, sector 1 = label
        let sector = &mut disk[512..1024];
        // label header
        sector[0..8].copy_from_slice(b"LABELONE");
        sector[8..16].copy_from_slice(&1u64.to_le_bytes()); // sector_number
                                                            // crc at 16..20, filled after
        sector[20..24].copy_from_slice(&32u32.to_le_bytes()); // data_offset
        sector[24..32].copy_from_slice(b"LVM2 001");

        // pv header at offset 32
        let uuid_bytes = format!("{:32}", pv_uuid); // pad to 32
        sector[32..64].copy_from_slice(&uuid_bytes.as_bytes()[..32]);
        sector[64..72].copy_from_slice(&pv_size.to_le_bytes());

        // one data area
        sector[72..80].copy_from_slice(&2048u64.to_le_bytes()); // offset
        sector[80..88].copy_from_slice(&(pv_size - 2048).to_le_bytes()); // size
                                                                         // terminator
                                                                         // (bytes 88..104 already zero)

        // one metadata area
        sector[104..112].copy_from_slice(&512u64.to_le_bytes()); // offset=512 (sector 1)
        sector[112..120].copy_from_slice(&(255 * 512u64).to_le_bytes()); // size
                                                                         // terminator
                                                                         // (bytes 120..136 already zero)

        // Compute and write CRC-32 of bytes 20..512
        let crc = crc::lvm_crc32(&sector[20..512]);
        sector[16..20].copy_from_slice(&crc.to_le_bytes());

        disk
    }

    fn fake_reader(data: Vec<u8>) -> impl Read + Seek {
        // Ensure at least 1024 bytes so sector 1 is readable
        let mut padded = data;
        if padded.len() < 1024 {
            padded.resize(1024, 0);
        }
        Cursor::new(padded)
    }

    #[test]
    fn parse_valid_label() {
        let disk = build_label_disk("9LBcEB7PQTGIlLI0KxrtzrynjuSL983W", 10_737_418_240);
        let mut reader = fake_reader(disk);
        let label = parse_pv_label(&mut reader, 0).unwrap();

        assert_eq!(label.pv_uuid, "9LBcEB7PQTGIlLI0KxrtzrynjuSL983W");
        assert_eq!(label.pv_size, 10_737_418_240);
        assert_eq!(label.data_areas.len(), 1);
        assert_eq!(label.data_areas[0].offset, 2048);
        assert_eq!(label.metadata_areas.len(), 1);
        assert_eq!(label.metadata_areas[0].offset, 512);
    }

    #[test]
    fn parse_label_at_partition_offset() {
        // PV starts at LBA 2048 (1 MB into the disk)
        let label_data = build_label_disk("abcd1234abcd1234abcd1234abcd1234", 5_000_000_000);
        // Need at least 2 sectors: padding + label sector (offset 0) + label sector (offset 512)
        let mut disk = vec![0u8; 2048 * 512]; // padding before PV
        disk.extend(label_data); // PV sector 0 + sector 1 (1024 bytes)

        let pv_offset = 2048 * 512;
        let mut reader = fake_reader(disk);
        let label = parse_pv_label(&mut reader, pv_offset).unwrap();

        assert_eq!(label.pv_uuid, "abcd1234abcd1234abcd1234abcd1234");
        assert_eq!(label.pv_size, 5_000_000_000);
    }

    #[test]
    fn reject_non_lvm_sector() {
        let mut disk = vec![0u8; 2048]; // at least 1024
        disk[512..520].copy_from_slice(b"NOTALABE"); // exactly 8 bytes
        let mut reader = fake_reader(disk);
        let err = parse_pv_label(&mut reader, 0).unwrap_err();
        assert!(matches!(err, LvmError::NotLvm));
    }

    #[test]
    fn reject_bad_crc() {
        let mut disk = build_label_disk("test1234test1234test1234test1234", 1_000_000);
        disk.resize(1024, 0);
        // Corrupt byte 600 (sector byte 88, well within CRC region, past magic bytes)
        disk[600] ^= 0xFF;
        let mut reader = fake_reader(disk);
        let err = parse_pv_label(&mut reader, 0).unwrap_err();
        assert!(matches!(err, LvmError::LabelCrcMismatch { .. }));
    }
}
