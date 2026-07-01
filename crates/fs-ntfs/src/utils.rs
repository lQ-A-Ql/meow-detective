//! Boot sector, MFT record, and low-level parsing utilities.

use crate::{invalid_fs_data, unexpected_fs_eof};
use evidence_core::EvidenceReader;
use std::io::{self, SeekFrom};

/// Apply the NTFS update sequence fixup to a FILE or INDX record.
pub(crate) fn apply_record_fixup(record: &mut [u8], sector_size: usize) -> io::Result<()> {
    if record.len() < 8 || sector_size < 2 {
        return Ok(());
    }

    let usa_offset = u16::from_le_bytes([record[4], record[5]]) as usize;
    let usa_count = u16::from_le_bytes([record[6], record[7]]) as usize;
    if usa_offset == 0 || usa_count < 2 {
        return Ok(());
    }

    let usa_bytes = usa_count
        .checked_mul(2)
        .ok_or_else(|| invalid_fs_data("invalid update sequence"))?;
    if usa_offset + usa_bytes > record.len() {
        return Err(invalid_fs_data(
            "update sequence array exceeds record length",
        ));
    }

    let expected = [record[usa_offset], record[usa_offset + 1]];
    for i in 1..usa_count {
        let fixup_pos = i
            .checked_mul(sector_size)
            .and_then(|v| v.checked_sub(2))
            .ok_or_else(|| invalid_fs_data("invalid fixup position"))?;
        if fixup_pos + 2 > record.len() {
            return Err(unexpected_fs_eof(
                "record too short for update sequence fixup",
            ));
        }

        if record[fixup_pos..fixup_pos + 2] != expected {
            return Err(invalid_fs_data("update sequence signature mismatch"));
        }

        let replacement = usa_offset + i * 2;
        record[fixup_pos] = record[replacement];
        record[fixup_pos + 1] = record[replacement + 1];
    }

    Ok(())
}

/// Validate that a buffer is a FILE record for the given inode.
pub(crate) fn validate_file_record(record: &[u8], inode: u64) -> io::Result<()> {
    if record.len() < 0x18 || &record[0..4] != b"FILE" {
        return Err(invalid_fs_data(format!(
            "inode {} is not a valid FILE record",
            inode
        )));
    }
    Ok(())
}

/// Return the base record reference stored in a FILE record header.
pub(crate) fn base_record_reference(record: &[u8]) -> u64 {
    if record.len() < 0x28 {
        return 0;
    }
    u64::from_le_bytes(record[0x20..0x28].try_into().unwrap_or([0; 8])) & 0x0000_FFFF_FFFF_FFFF
}

/// Check whether `record` is an extension record for `base_inode`.
pub(crate) fn is_extension_record_for(record: &[u8], base_inode: u64) -> bool {
    record.len() >= 0x28 && &record[0..4] == b"FILE" && base_record_reference(record) == base_inode
}

/// Extract an inode from an `mft:` prefixed path.
pub(crate) fn mft_inode_from_path(path: &str) -> Option<u64> {
    path.strip_prefix("mft:")
        .and_then(|s| s.rsplit(':').next()?.parse::<u64>().ok())
}

// --- Boot sector parsing helpers ---

pub(crate) fn root_dir_frn(boot: &[u8]) -> u64 {
    let mft_ref = u64::from_le_bytes(boot[0x2C..0x34].try_into().unwrap_or([0; 8]));
    mft_ref & 0x0000_FFFF_FFFF_FFFF
}

pub(crate) fn mft_record_bytes(boot: &[u8]) -> u32 {
    let raw = boot[0x40] as i8;
    if raw > 0 {
        1024
    } else if raw < 0 {
        let shift = (raw as i16).unsigned_abs();
        if shift < 32 {
            (1u32 << shift).max(512)
        } else {
            1024
        }
    } else {
        1024
    }
}

pub(crate) fn index_record_bytes(boot: &[u8], cluster_size: u32, fallback: u32) -> u32 {
    let raw = boot[0x44] as i8;
    if raw > 0 {
        let bytes = cluster_size.saturating_mul(raw as u32);
        if bytes >= 512 {
            bytes
        } else {
            fallback
        }
    } else if raw < 0 {
        let shift = (raw as i16).unsigned_abs();
        if shift < 32 {
            (1u32 << shift).max(512)
        } else {
            fallback
        }
    } else {
        fallback
    }
}

pub(crate) fn read_contiguous_mft_record(
    reader: &mut dyn EvidenceReader,
    volume_offset: u64,
    mft_cluster: u64,
    cluster_size: u64,
    record_size: u32,
    bytes_per_sector: u16,
    record_number: u64,
) -> io::Result<Vec<u8>> {
    let offset = volume_offset
        .checked_add(mft_cluster.saturating_mul(cluster_size))
        .and_then(|base| base.checked_add(record_number.saturating_mul(record_size as u64)))
        .ok_or_else(|| invalid_fs_data("MFT record offset overflow"))?;
    let mut rec = vec![0u8; record_size as usize];
    reader.seek(SeekFrom::Start(offset))?;
    reader.read_exact(&mut rec)?;
    apply_record_fixup(&mut rec, bytes_per_sector as usize)?;
    Ok(rec)
}

/// Parse the $DATA non-resident real_size from MFT record 0.
/// Used by E01 import/enumeration code to determine $MFT data size.
pub fn parse_mft_data_real_size(record: &[u8]) -> Option<u64> {
    if &record[0..4] != b"FILE" {
        return None;
    }
    let attr_off = u16::from_le_bytes([record[0x14], record[0x15]]) as usize;
    let mut pos = attr_off;
    while pos + 8 < record.len() {
        let typ = u32::from_le_bytes(record[pos..pos + 4].try_into().ok()?);
        if typ == crate::ATTR_TYPE_END {
            break;
        }
        let len = u32::from_le_bytes(record[pos + 4..pos + 8].try_into().ok()?) as usize;
        if len < 4 || pos + len > record.len() {
            break;
        }
        // $DATA non-resident (0x80) with non-resident flag bit 0 set
        if typ == crate::ATTR_TYPE_DATA && pos + 0x38 <= record.len() && (record[pos + 8] & 1) != 0
        {
            return Some(u64::from_le_bytes(
                record[pos + 0x30..pos + 0x38].try_into().ok()?,
            ));
        }
        if len == 0 {
            break;
        }
        pos += len;
    }
    None
}

#[test]
fn mft_inode_fast_path_handles_partition_record_format() {
    // "mft:3:42" format from parallel MFT enumeration
    let path = "mft:3:42";
    let inode = mft_inode_from_path(path);
    assert_eq!(inode, Some(42));
}

#[test]
fn mft_inode_fast_path_handles_legacy_format() {
    // "mft:5" format from legacy MFT enumeration
    let path = "mft:5";
    let inode = mft_inode_from_path(path);
    assert_eq!(inode, Some(5));
}
