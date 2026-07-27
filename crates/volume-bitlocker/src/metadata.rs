//! FVE metadata block and entry parsing.
//!
//! Derived from `bitlocker-core`'s `metadata` module (see `../NOTICE`).
//!
//! A BitLocker volume carries three copies of an FVE metadata block. Each is a
//! `-FVE-FS-` block header, a 48-byte metadata header holding the cipher and
//! volume identity, and a recursive array of metadata entries: the key
//! protectors, the wrapped FVEK, and the relocated volume-header descriptor.
//!
//! Three copies exist so that a damaged block does not lose the volume. Callers
//! must try every non-zero offset before declaring the metadata unreadable.

use crate::bytes::{le_u16, le_u32, le_u64, read_guid, slice_owned};
use crate::method::EncryptionMethod;
use crate::protector::{ProtectorInventory, ProtectorKind};

const FVE_SIGNATURE: &[u8; 8] = b"-FVE-FS-";

/// Entry type: a volume master key protector.
pub(crate) const ENTRY_TYPE_VMK: u16 = 0x0002;
/// Entry type: the wrapped full volume encryption key.
pub(crate) const ENTRY_TYPE_FVEK: u16 = 0x0003;
/// Entry type and value type: the relocated volume-header descriptor.
pub(crate) const ENTRY_TYPE_VOLUME_HEADER: u16 = 0x000f;

// Value type 0x0001 is a raw key: a 4-byte method then the key bytes. It holds
// the clear key of a clear-key protector, which v1 reports but never uses, so
// there is no production reader for it and no constant here. The synthetic-volume
// builder writes the literal directly.
/// Value type: a stretch key (a 4-byte method, then a 16-byte salt).
pub(crate) const VALUE_TYPE_STRETCH: u16 = 0x0003;
/// Value type: an AES-CCM wrapped key (`nonce(12) | tag(16) | ciphertext`).
pub(crate) const VALUE_TYPE_AES_CCM: u16 = 0x0005;
/// Value type: a volume master key protector.
pub(crate) const VALUE_TYPE_VMK: u16 = 0x0008;

/// Protector code: clear key. The VMK is stored unwrapped.
pub(crate) const PROTECTION_CLEAR: u16 = 0x0000;
/// Protector code: external startup key, held in a `.BEK` file.
pub(crate) const PROTECTION_STARTUP_KEY: u16 = 0x0200;
/// Protector code: TPM.
pub(crate) const PROTECTION_TPM: u16 = 0x0100;
/// Protector code: TPM with a PIN.
pub(crate) const PROTECTION_TPM_PIN: u16 = 0x0400;
/// Protector code: 48-digit recovery password.
pub(crate) const PROTECTION_RECOVERY: u16 = 0x0800;
/// Protector code: user password.
pub(crate) const PROTECTION_PASSWORD: u16 = 0x2000;

/// Classifies an on-disk protector code.
pub(crate) fn classify_protector(code: u16) -> ProtectorKind {
    match code {
        PROTECTION_CLEAR => ProtectorKind::ClearKey,
        PROTECTION_TPM => ProtectorKind::Tpm,
        PROTECTION_TPM_PIN => ProtectorKind::Tpm,
        PROTECTION_STARTUP_KEY => ProtectorKind::StartupKey,
        PROTECTION_RECOVERY => ProtectorKind::RecoveryPassword,
        PROTECTION_PASSWORD => ProtectorKind::Password,
        other => ProtectorKind::Unknown(other),
    }
}

/// One FVE metadata entry: its type triple and the value data that follows the
/// 8-byte entry header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetadataEntry {
    /// Entry type, for example [`ENTRY_TYPE_VMK`].
    pub entry_type: u16,
    /// Value type, for example [`VALUE_TYPE_AES_CCM`].
    pub value_type: u16,
    /// Entry version, typically 1.
    pub version: u16,
    /// The value data following the 8-byte entry header.
    pub data: Vec<u8>,
}

impl MetadataEntry {
    /// Parses a flat sequence of metadata entries.
    ///
    /// Each entry is `size(u16) | entry_type(u16) | value_type(u16) |
    /// version(u16) | value_data`. Parsing stops on a size below the 8-byte
    /// header, an entry that would run past the buffer, or the end of the input.
    /// A size of zero or a lying size therefore terminates the walk instead of
    /// looping forever — the input is untrusted.
    #[must_use]
    pub fn parse_sequence(data: &[u8]) -> Vec<Self> {
        let mut entries = Vec::new();
        let mut position = 0usize;
        while position + 8 <= data.len() {
            let size = le_u16(data, position) as usize;
            if size < 8 || position + size > data.len() {
                break;
            }
            entries.push(Self {
                entry_type: le_u16(data, position + 2),
                value_type: le_u16(data, position + 4),
                version: le_u16(data, position + 6),
                data: slice_owned(data, position + 8, size - 8),
            });
            position += size;
        }
        entries
    }

    /// Parses this entry's value data from `offset` as a nested entry sequence.
    #[must_use]
    pub fn nested(&self, offset: usize) -> Vec<Self> {
        let start = offset.min(self.data.len());
        Self::parse_sequence(&self.data[start..])
    }

    /// Whether this entry is a volume master key protector.
    #[must_use]
    pub fn is_vmk(&self) -> bool {
        self.entry_type == ENTRY_TYPE_VMK && self.value_type == VALUE_TYPE_VMK
    }

    /// The raw protector code of a VMK entry, at value offset 26.
    #[must_use]
    pub fn protection_code(&self) -> Option<u16> {
        self.is_vmk().then(|| le_u16(&self.data, 26))
    }
}

/// A parsed FVE metadata block.
#[derive(Debug, Clone)]
pub struct FveMetadata {
    /// The volume encryption method.
    pub encryption_method: EncryptionMethod,
    /// The raw encryption-method code, preserved even when unrecognized.
    pub encryption_method_code: u16,
    /// Volume identifier GUID.
    pub volume_guid: [u8; 16],
    /// Volume creation time as a Windows FILETIME.
    pub creation_time: u64,
    /// The metadata entries, in on-disk order.
    pub entries: Vec<MetadataEntry>,
    /// Still-encrypted bytes from the front of the volume; zero means the whole
    /// volume is encrypted.
    pub encrypted_volume_size: u64,
    /// Byte offset where the original volume header is stored, relocated.
    pub volume_header_offset: u64,
    /// Size in bytes of the relocated volume-header region.
    pub volume_header_size: u64,
    /// Byte offsets of the three metadata blocks, as recorded in this block.
    pub metadata_offsets: [u64; 3],
    /// Size of the metadata region.
    pub metadata_size: u32,
}

impl FveMetadata {
    /// Parses an FVE metadata block starting at its block header.
    ///
    /// Returns `None` when the `-FVE-FS-` block signature is absent, so a caller
    /// walking the three copies can move to the next offset. This is also what
    /// disambiguates a real BitLocker To Go volume from plain FAT: the `MSWIN4.1`
    /// header signature is shared, but only an encrypted volume has this block.
    #[must_use]
    pub fn parse(block: &[u8], bytes_per_sector: u16) -> Option<Self> {
        if block.get(0..8) != Some(FVE_SIGNATURE.as_slice()) {
            return None;
        }

        let encrypted_volume_size = le_u64(block, 16);
        let volume_header_sectors = le_u32(block, 28);
        let metadata_offsets = [le_u64(block, 32), le_u64(block, 40), le_u64(block, 48)];
        let block_volume_header_offset = le_u64(block, 56);

        // The FVE metadata header starts at block offset 64.
        let header = 64usize;
        let metadata_size = le_u32(block, header);
        let volume_guid = read_guid(block, header + 16);
        let encryption_method_code = le_u16(block, header + 36);
        let creation_time = le_u64(block, header + 40);

        // Entries follow the 48-byte metadata header, bounded by metadata_size so
        // an oversized field cannot walk past the block we actually read.
        let entries_start = header + 48;
        let entries_end = header
            .saturating_add(metadata_size as usize)
            .min(block.len());
        let entries = if entries_end > entries_start {
            MetadataEntry::parse_sequence(&block[entries_start..entries_end])
        } else {
            Vec::new()
        };

        let (volume_header_offset, volume_header_size) = resolve_volume_header_region(
            &entries,
            block_volume_header_offset,
            u64::from(volume_header_sectors) * u64::from(bytes_per_sector),
        );

        Some(Self {
            encryption_method: EncryptionMethod::from_code(encryption_method_code),
            encryption_method_code,
            volume_guid,
            creation_time,
            entries,
            encrypted_volume_size,
            volume_header_offset,
            volume_header_size,
            metadata_offsets,
            metadata_size,
        })
    }

    /// The VMK protector entries.
    pub fn vmk_entries(&self) -> impl Iterator<Item = &MetadataEntry> {
        self.entries.iter().filter(|entry| entry.is_vmk())
    }

    /// The wrapped FVEK entry, if present.
    #[must_use]
    pub fn fvek_entry(&self) -> Option<&MetadataEntry> {
        self.entries
            .iter()
            .find(|entry| entry.entry_type == ENTRY_TYPE_FVEK)
    }

    /// Every protector on the volume, in on-disk order.
    ///
    /// This is the forensic answer to "what could unlock this volume", and stays
    /// available even when nothing here can unlock it.
    #[must_use]
    pub fn protector_inventory(&self) -> ProtectorInventory {
        ProtectorInventory::new(
            self.vmk_entries()
                .filter_map(MetadataEntry::protection_code)
                .map(classify_protector)
                .collect(),
        )
    }

    /// The raw protector codes, for diagnostics that must not lose an unknown value.
    #[must_use]
    pub fn protector_codes(&self) -> Vec<u16> {
        self.vmk_entries()
            .filter_map(MetadataEntry::protection_code)
            .collect()
    }
}

/// Resolves the relocated volume-header region.
///
/// The dedicated descriptor entry wins over the block-header fields when it
/// carries non-zero values, matching how the reference implementations resolve a
/// volume whose block header and descriptor disagree.
fn resolve_volume_header_region(
    entries: &[MetadataEntry],
    block_offset: u64,
    block_size: u64,
) -> (u64, u64) {
    let mut offset = block_offset;
    let mut size = block_size;
    if let Some(entry) = entries
        .iter()
        .find(|entry| entry.entry_type == ENTRY_TYPE_VOLUME_HEADER)
    {
        let descriptor_offset = le_u64(&entry.data, 0);
        let descriptor_size = le_u64(&entry.data, 8);
        if descriptor_offset != 0 {
            offset = descriptor_offset;
        }
        if descriptor_size != 0 {
            size = descriptor_size;
        }
    }
    (offset, size)
}

#[cfg(test)]
#[path = "../tests/unit/metadata.rs"]
mod tests;
