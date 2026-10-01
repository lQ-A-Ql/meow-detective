//! Read-only discovery of complete metadata copies.
use super::evidence_io::read_exact_at;
use crate::bytes::le_u32;
use crate::metadata::{BLOCK_HEADER_LEN, MAX_METADATA_ENTRIES_LEN, METADATA_HEADER_LEN};
use crate::{BitLockerError, FveMetadata, Result, VolumeHeader};
use std::io::{Read, Seek};
const HEADER_LEN: usize = 512;
const METADATA_PREFIX_LEN: usize = BLOCK_HEADER_LEN + METADATA_HEADER_LEN;
/// What a locked volume reveals without any credential.
#[derive(Debug, Clone)]
pub struct VolumeIdentity {
    /// The parsed metadata block.
    pub metadata: FveMetadata,
    /// Bytes per sector, from the volume header.
    pub bytes_per_sector: u16,
}

/// Reads the volume header and the first valid FVE metadata block.
///
/// Tries every non-zero metadata offset before failing, because the three copies
/// exist precisely so a damaged block does not lose the volume.
///
/// A successful return is also what confirms the volume really is BitLocker: a
/// `MSWIN4.1` header alone is ambiguous with plain FAT, and only the `-FVE-FS-`
/// metadata block settles it.
///
/// # Errors
///
/// [`BitLockerError::MetadataUnreadable`] when the header signature is absent or
/// no candidate offset yields a valid block; [`BitLockerError::EvidenceRead`] when
/// the underlying reader fails.
pub fn read_volume_identity<R: Read + Seek>(reader: &mut R) -> Result<VolumeIdentity> {
    read_volume_identities(reader)?
        .into_iter()
        .next()
        .ok_or_else(|| BitLockerError::MetadataUnreadable {
            reason: "the volume header contains no non-zero metadata offsets".to_string(),
        })
}

/// Reads every structurally valid FVE metadata copy reachable from the volume
/// header. A failed seek, short read, or malformed copy is isolated to that copy.
///
/// # Errors
///
/// Returns an evidence-read error when the volume header itself is unavailable,
/// or metadata-unreadable after all non-zero copies fail validation.
pub fn read_volume_identities<R: Read + Seek>(reader: &mut R) -> Result<Vec<VolumeIdentity>> {
    let mut header_sector = [0u8; HEADER_LEN];
    read_exact_at(reader, 0, &mut header_sector)?;
    let header = VolumeHeader::parse(&header_sector)?;

    let mut offsets = header
        .fve_metadata_offsets
        .into_iter()
        .filter(|offset| *offset != 0)
        .collect::<Vec<_>>();
    offsets.dedup();
    let mut identities = Vec::new();
    let mut failures = Vec::new();
    let mut index = 0usize;
    while index < offsets.len() {
        let offset = offsets[index];
        index += 1;
        match read_metadata_copy(reader, offset, header.bytes_per_sector) {
            Ok(metadata) => {
                for discovered in metadata.metadata_offsets {
                    if discovered != 0 && !offsets.contains(&discovered) {
                        offsets.push(discovered);
                    }
                }
                identities.push(VolumeIdentity {
                    metadata,
                    bytes_per_sector: header.bytes_per_sector,
                });
            }
            Err(error) => failures.push(format!("{offset:#X}: {error}")),
        }
    }

    if identities.is_empty() {
        return Err(BitLockerError::MetadataUnreadable {
            reason: format!(
                "no complete v2 metadata block at candidate offsets {offsets:?}; {}",
                failures.join("; ")
            ),
        });
    }
    Ok(identities)
}

fn read_metadata_copy<R: Read + Seek>(
    reader: &mut R,
    offset: u64,
    bytes_per_sector: u16,
) -> Result<FveMetadata> {
    let mut prefix = [0u8; METADATA_PREFIX_LEN];
    read_exact_at(reader, offset, &mut prefix)?;
    let metadata_size = le_u32(&prefix, BLOCK_HEADER_LEN) as usize;
    let entries_len = metadata_size
        .checked_sub(METADATA_HEADER_LEN)
        .ok_or_else(|| BitLockerError::MetadataUnreadable {
            reason: format!("metadata copy at {offset:#X} has a header smaller than 48 bytes"),
        })?;
    if entries_len > MAX_METADATA_ENTRIES_LEN {
        return Err(BitLockerError::MetadataUnreadable {
            reason: format!(
                "metadata copy at {offset:#X} declares {entries_len} entry bytes; maximum is {MAX_METADATA_ENTRIES_LEN}"
            ),
        });
    }
    let total_len = BLOCK_HEADER_LEN.checked_add(metadata_size).ok_or_else(|| {
        BitLockerError::MetadataUnreadable {
            reason: format!("metadata copy at {offset:#X} has an overflowing size"),
        }
    })?;
    let mut block = vec![0u8; total_len];
    read_exact_at(reader, offset, &mut block)?;
    FveMetadata::parse(&block, bytes_per_sector).ok_or_else(|| BitLockerError::MetadataUnreadable {
        reason: format!("metadata copy at {offset:#X} failed strict v2 validation"),
    })
}
