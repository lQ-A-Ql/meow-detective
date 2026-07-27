//! BitLocker volume-header parsing.
//!
//! Derived from `bitlocker-core`'s `header` module (see `../NOTICE`).
//!
//! The first 512-byte sector identifies the on-disk variant and locates the
//! three FVE metadata blocks. Three layouts exist — Windows Vista, Windows 7 and
//! later, and BitLocker To Go on FAT — distinguished by the signature at offset 3
//! and the boot entry at offset 0.
//!
//! # The `MSWIN4.1` trap
//!
//! `MSWIN4.1` at offset 3 is *not* proof of BitLocker. It is the ordinary OEM
//! name on plain FAT volumes formatted by Windows, so a header parse alone would
//! misclassify every such volume as encrypted. Only the `-FVE-FS-` signature is
//! self-identifying. A `BitLockerToGo` header is therefore a *candidate* and must
//! be confirmed by a valid FVE metadata block before anything reports the volume
//! as BitLocker; see [`crate::metadata::FveMetadata`] and the
//! `read_volume_metadata` entry point.

use crate::bytes::{le_u16, le_u64, read_guid};
use crate::error::{BitLockerError, Result};

/// Which BitLocker on-disk volume-header layout was recognized.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeaderVariant {
    /// Windows Vista (`-FVE-FS-`, boot `EB 52 90`). Metadata block 1 is a cluster
    /// number; the other two offsets come from the metadata block header itself.
    WindowsVista,
    /// Windows 7 and later (`-FVE-FS-`, boot `EB 58 90`).
    Windows7OrLater,
    /// BitLocker To Go on a FAT volume (`MSWIN4.1`).
    ///
    /// Self-identifying only in combination with valid FVE metadata — plain FAT
    /// carries the same signature.
    BitLockerToGoCandidate,
}

impl HeaderVariant {
    /// Whether the signature alone is sufficient to claim the volume is BitLocker.
    ///
    /// False for [`Self::BitLockerToGoCandidate`], where `MSWIN4.1` is ambiguous
    /// with plain FAT and metadata validation is mandatory.
    #[must_use]
    pub fn is_self_identifying(self) -> bool {
        matches!(self, Self::WindowsVista | Self::Windows7OrLater)
    }
}

/// The parsed BitLocker volume header.
#[derive(Debug, Clone)]
pub struct VolumeHeader {
    /// Which layout was recognized.
    pub variant: HeaderVariant,
    /// Bytes per sector from the BPB; falls back to 512 when the field is zero.
    pub bytes_per_sector: u16,
    /// The BitLocker identifier GUID. All zeros for the Vista layout, which
    /// stores none at a fixed offset.
    pub bitlocker_guid: [u8; 16],
    /// Byte offsets of the three FVE metadata blocks, relative to volume start.
    ///
    /// For the Vista layout only the first is derived here; that block's own
    /// header carries all three authoritatively.
    pub fve_metadata_offsets: [u64; 3],
}

const SIGNATURE_FVE: &[u8; 8] = b"-FVE-FS-";
const SIGNATURE_TO_GO: &[u8; 8] = b"MSWIN4.1";
const BOOT_ENTRY_WINDOWS7: [u8; 3] = [0xeb, 0x58, 0x90];

impl VolumeHeader {
    /// Parses the 512-byte volume header sector.
    ///
    /// # Errors
    ///
    /// [`BitLockerError::MetadataUnreadable`] when neither the `-FVE-FS-` nor the
    /// `MSWIN4.1` signature is present. A short buffer takes the same path rather
    /// than panicking.
    pub fn parse(sector: &[u8]) -> Result<Self> {
        let mut signature = [0u8; 8];
        if let Some(slice) = sector.get(3..11) {
            signature.copy_from_slice(slice);
        }
        let mut bytes_per_sector = le_u16(sector, 11);
        if bytes_per_sector == 0 {
            bytes_per_sector = 512;
        }

        if &signature == SIGNATURE_TO_GO {
            return Ok(Self {
                variant: HeaderVariant::BitLockerToGoCandidate,
                bytes_per_sector,
                bitlocker_guid: read_guid(sector, 424),
                fve_metadata_offsets: [
                    le_u64(sector, 440),
                    le_u64(sector, 448),
                    le_u64(sector, 456),
                ],
            });
        }

        if &signature == SIGNATURE_FVE {
            if sector.get(0..3) == Some(BOOT_ENTRY_WINDOWS7.as_slice()) {
                return Ok(Self {
                    variant: HeaderVariant::Windows7OrLater,
                    bytes_per_sector,
                    bitlocker_guid: read_guid(sector, 160),
                    fve_metadata_offsets: [
                        le_u64(sector, 176),
                        le_u64(sector, 184),
                        le_u64(sector, 192),
                    ],
                });
            }
            return Ok(Self::parse_vista(sector, bytes_per_sector));
        }

        Err(BitLockerError::MetadataUnreadable {
            reason: format!(
                "volume header signature is {:?}, expected -FVE-FS- or MSWIN4.1",
                String::from_utf8_lossy(&signature)
            ),
        })
    }

    /// Builds the Vista-layout header, where metadata block 1 is a cluster number.
    fn parse_vista(sector: &[u8], bytes_per_sector: u16) -> Self {
        // Cluster size is bytes-per-sector times sectors-per-cluster (offset 13).
        // A zero sectors-per-cluster is clamped to 1 so a corrupt BPB yields a
        // wrong-but-bounded offset that metadata validation then rejects.
        let sectors_per_cluster = u64::from(sector.get(13).copied().unwrap_or(1).max(1));
        let cluster_size = u64::from(bytes_per_sector) * sectors_per_cluster;
        let first_block = le_u64(sector, 56).saturating_mul(cluster_size);
        Self {
            variant: HeaderVariant::WindowsVista,
            bytes_per_sector,
            bitlocker_guid: [0u8; 16],
            fve_metadata_offsets: [first_block, 0, 0],
        }
    }
}

#[cfg(test)]
#[path = "../tests/unit/header.rs"]
mod tests;
