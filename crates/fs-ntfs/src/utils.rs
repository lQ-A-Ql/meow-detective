//! Boot sector, MFT record, and low-level parsing utilities.

use crate::{invalid_fs_data, unexpected_fs_eof};
use evidence_core::EvidenceReader;
use std::io::{self, SeekFrom};

/// Apply the NTFS update sequence fixup to a FILE or INDX record.
pub fn apply_record_fixup(record: &mut [u8], sector_size: usize) -> io::Result<()> {
    if record.len() < 8 || sector_size < 2 {
        return Ok(());
    }

    let usa_offset = u16::from_le_bytes([record[4], record[5]]) as usize;
    let usa_count = u16::from_le_bytes([record[6], record[7]]) as usize;
    if usa_offset == 0 || usa_count < 2 {
        return Ok(());
    }
    // NTFS multi-sector protection uses 512-byte blocks, including 4Kn volumes.
    let stride = 512usize;

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
            .checked_mul(stride)
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
    }
    for i in 1..usa_count {
        let fixup_pos = i * stride - 2;
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

/// Return the full base file reference stored in a FILE record header.
pub(crate) fn base_record_reference(record: &[u8]) -> u64 {
    if record.len() < 0x28 {
        return 0;
    }
    u64::from_le_bytes(record[0x20..0x28].try_into().unwrap_or([0; 8]))
}

pub(crate) fn file_record_sequence(record: &[u8]) -> Option<u16> {
    (record.len() >= 0x12).then(|| u16::from_le_bytes([record[0x10], record[0x11]]))
}

/// Check the full record number and sequence in an extension's base reference.
pub(crate) fn is_extension_record_for(record: &[u8], base_inode: u64, base_sequence: u16) -> bool {
    let expected = (u64::from(base_sequence) << 48) | (base_inode & 0x0000_FFFF_FFFF_FFFF);
    record.len() >= 0x28 && &record[0..4] == b"FILE" && base_record_reference(record) == expected
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

pub fn mft_record_bytes(boot: &[u8]) -> io::Result<u32> {
    let raw = *boot
        .get(0x40)
        .ok_or_else(|| unexpected_fs_eof("NTFS boot sector"))? as i8;
    let bytes = if raw > 0 {
        let sector_size = u32::from(u16::from_le_bytes([boot[11], boot[12]]));
        sector_size
            .checked_mul(u32::from(boot[13]))
            .and_then(|cluster| cluster.checked_mul(raw as u32))
    } else if raw < 0 {
        let shift = (raw as i16).unsigned_abs();
        1u32.checked_shl(u32::from(shift))
    } else {
        None
    };
    let bytes = bytes
        .filter(|size| *size >= 512 && size.is_power_of_two())
        .ok_or_else(|| invalid_fs_data("invalid NTFS MFT record size"))?;
    if bytes > 16 * 1024 * 1024 {
        return Err(evidence_core::filesystem::fs_out_of_memory(
            "NTFS MFT record exceeds 16 MiB",
        ));
    }
    Ok(bytes)
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
    let offset = mft_cluster
        .checked_mul(cluster_size)
        .and_then(|offset| volume_offset.checked_add(offset))
        .and_then(|base| {
            record_number
                .checked_mul(u64::from(record_size))
                .and_then(|offset| base.checked_add(offset))
        })
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
    crate::data_runs::parse_mft_data_attribute(record)
        .ok()
        .map(|data| data.real_size)
}

#[cfg(test)]
#[path = "../tests/unit/utils.rs"]
mod tests;
