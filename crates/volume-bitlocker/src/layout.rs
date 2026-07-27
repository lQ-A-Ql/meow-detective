//! Where a logical volume offset actually lives, and whether it is encrypted.
//!
//! Derived from `bitlocker-core`'s read-path region handling (see `../NOTICE`).
//!
//! A BitLocker volume's plaintext view is not a straight decryption of the
//! ciphertext. Three rules reshape it, and all three are pure address arithmetic
//! with no key material, which is why they live apart from the cipher and can be
//! tested without one.
//!
//! 1. The original volume header was moved aside to make room for the BitLocker
//!    header, so logical offsets below `volume_header_size` are stored at
//!    `volume_header_offset + offset`.
//! 2. The three FVE metadata blocks are not part of the filesystem. They read
//!    back as zeros rather than as decrypted metadata.
//! 3. On a partially-encrypted volume, everything at or past
//!    `encrypted_volume_size` is stored as plaintext already and must not be run
//!    through the cipher.

use crate::cipher::CIPHER_SECTOR_SIZE;
use crate::metadata::FveMetadata;

/// Encryption-method code meaning "not encrypted".
const METHOD_NONE: u16 = 0x0000;

/// What a logical sector maps to in the ciphertext.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SectorSource {
    /// Read from `physical_offset` and decrypt at that same offset.
    Encrypted { physical_offset: u64 },
    /// Read from `physical_offset` and return the bytes as they are.
    Plaintext { physical_offset: u64 },
    /// Not part of the plaintext view; return zeros without reading.
    Blanked,
}

/// The address-space facts needed to serve a plaintext read.
///
/// Carries no key material, so it is freely `Debug` and `Clone` — unlike
/// everything in [`crate::secret`].
#[derive(Debug, Clone)]
pub struct VolumeLayout {
    /// Byte offsets of the three FVE metadata blocks. Zero means absent.
    metadata_offsets: [u64; 3],
    /// Size of each metadata block region.
    metadata_size: u64,
    /// Where the relocated original volume header is stored.
    volume_header_offset: u64,
    /// How many bytes of the volume start are relocated.
    volume_header_size: u64,
    /// Bytes still encrypted from the front; zero means the whole volume.
    encrypted_volume_size: u64,
    /// Whether the volume is encrypted at all.
    encrypted: bool,
}

impl VolumeLayout {
    /// Extracts the layout from parsed metadata.
    #[must_use]
    pub fn from_metadata(metadata: &FveMetadata) -> Self {
        Self {
            metadata_offsets: metadata.metadata_offsets,
            metadata_size: u64::from(metadata.metadata_size),
            volume_header_offset: metadata.volume_header_offset,
            volume_header_size: metadata.volume_header_size,
            encrypted_volume_size: metadata.encrypted_volume_size,
            encrypted: metadata.encryption_method_code != METHOD_NONE,
        }
    }

    /// Resolves one logical sector start to its source in the ciphertext.
    ///
    /// `logical_start` must be a multiple of [`CIPHER_SECTOR_SIZE`].
    pub(crate) fn resolve(&self, logical_start: u64) -> SectorSource {
        if self.is_metadata_region(logical_start) {
            return SectorSource::Blanked;
        }

        // Below the relocated-header size, the bytes live elsewhere — and the
        // physical location is also the cipher's address for them. Using the
        // logical offset as the IV here decrypts to garbage with no error.
        let physical_offset = if logical_start < self.volume_header_size {
            self.volume_header_offset.saturating_add(logical_start)
        } else {
            logical_start
        };

        if !self.encrypted || self.is_past_encrypted_region(physical_offset) {
            return SectorSource::Plaintext { physical_offset };
        }
        SectorSource::Encrypted { physical_offset }
    }

    /// Whether a logical sector falls inside any FVE metadata block.
    fn is_metadata_region(&self, logical_start: u64) -> bool {
        self.metadata_offsets.iter().any(|&start| {
            start != 0
                && logical_start >= start
                && logical_start < start.saturating_add(self.metadata_size)
        })
    }

    /// Whether a physical offset lies past the still-encrypted region.
    ///
    /// A zero `encrypted_volume_size` means the whole volume is encrypted, not
    /// that none of it is — inverting this leaves a fully encrypted volume
    /// reading back as ciphertext.
    fn is_past_encrypted_region(&self, physical_offset: u64) -> bool {
        self.encrypted_volume_size != 0 && physical_offset >= self.encrypted_volume_size
    }

    /// Whether the volume carries an encryption method at all.
    #[must_use]
    pub fn is_encrypted(&self) -> bool {
        self.encrypted
    }

    /// The size of the relocated volume-header region.
    #[must_use]
    pub fn volume_header_size(&self) -> u64 {
        self.volume_header_size
    }
}

/// Rounds a byte offset down to its cipher sector boundary.
pub(crate) fn sector_start(offset: u64) -> u64 {
    offset - (offset % CIPHER_SECTOR_SIZE as u64)
}

#[cfg(test)]
#[path = "../tests/unit/layout.rs"]
mod tests;
