//! Read-only generated MBR disk selecting one verified direct NTFS volume.
use crate::{CowDisk, EmulationError};
use evidence_block::{BlockDeviceError, BlockProvider};
use std::sync::Arc;

pub const NTFS_GUEST_VOLUME_OFFSET: u64 = 2048 * 512;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceVolumeMapping {
    pub source_media_length: u64,
    pub source_volume_length: u64,
    pub guest_volume_offset: u64,
    pub disk_signature: u32,
    pub original_hidden_sectors: u32,
}

impl SourceVolumeMapping {
    pub fn guest_length(self) -> Result<u64, EmulationError> {
        self.guest_volume_offset
            .checked_add(self.source_volume_length)
            .ok_or(EmulationError::ArithmeticOverflow)
    }
    pub fn partition_offset(self, source_offset: u64) -> Result<u64, EmulationError> {
        if source_offset != 0 {
            return Err(EmulationError::InvalidDerivedVolume(
                "the selected source volume starts at zero",
            ));
        }
        Ok(self.guest_volume_offset)
    }
    /// Seed only derived BPB fields. All file bytes and source evidence stay intact.
    pub fn seed_guest_boot_parameters(self, disk: &CowDisk) -> Result<(), EmulationError> {
        if disk.source_volume_mapping() != Some(self) {
            return Err(EmulationError::ParentMismatch);
        }
        let hidden = u32::try_from(self.guest_volume_offset / 512)
            .map_err(|_| EmulationError::ArithmeticOverflow)?
            .to_le_bytes();
        disk.write_all_at(self.guest_volume_offset + 28, &hidden)?;
        disk.write_all_at(self.guest_length()? - 512 + 28, &hidden)?;
        disk.flush()
    }
}

pub struct NtfsBootDisk {
    parent: Arc<dyn BlockProvider>,
    mapping: SourceVolumeMapping,
    mbr: [u8; 512],
    length: u64,
}

impl NtfsBootDisk {
    pub fn new(
        parent: Arc<dyn BlockProvider>,
        volume_length: u64,
        disk_signature: u32,
    ) -> Result<Self, EmulationError> {
        if disk_signature == 0
            || volume_length < 1024
            || !volume_length.is_multiple_of(512)
            || volume_length / 512 > u64::from(u32::MAX) - 2048
            || volume_length > parent.len()
        {
            return Err(EmulationError::InvalidDerivedVolume(
                "MBR selection requires a nonzero signature and a bounded 512-byte volume",
            ));
        }
        let mut boot = [0; 512];
        parent.read_exact_at(0, &mut boot)?;
        validate_boot(&parent, &boot, volume_length)?;
        let mapping = SourceVolumeMapping {
            source_media_length: parent.len(),
            source_volume_length: volume_length,
            guest_volume_offset: NTFS_GUEST_VOLUME_OFFSET,
            disk_signature,
            original_hidden_sectors: u32::from_le_bytes(boot[28..32].try_into().unwrap_or([0; 4])),
        };
        let length = mapping.guest_length()?;
        let mbr = make_mbr(mapping);
        Ok(Self {
            parent,
            mapping,
            mbr,
            length,
        })
    }
    pub fn mapping(&self) -> SourceVolumeMapping {
        self.mapping
    }
    pub fn mbr(&self) -> &[u8; 512] {
        &self.mbr
    }
}

fn validate_boot(
    parent: &Arc<dyn BlockProvider>,
    boot: &[u8; 512],
    length: u64,
) -> Result<(), EmulationError> {
    let sectors = u64::from_le_bytes(boot[40..48].try_into().unwrap_or([0; 8]));
    let declared = sectors
        .checked_mul(512)
        .ok_or(EmulationError::ArithmeticOverflow)?;
    if &boot[3..11] != b"NTFS    "
        || boot[11..13] != 512u16.to_le_bytes()
        || boot[510..512] != [0x55, 0xaa]
        || !boot[13].is_power_of_two()
        || !(length == declared || declared.checked_add(512) == Some(length))
    {
        return Err(EmulationError::InvalidDerivedVolume(
            "a verified 512-byte NTFS extent is required",
        ));
    }
    let mut backup = [0; 512];
    parent.read_exact_at(length - 512, &mut backup)?;
    if backup != *boot {
        return Err(EmulationError::InvalidDerivedVolume(
            "the selected extent must end with its matching NTFS backup boot sector",
        ));
    }
    Ok(())
}

fn make_mbr(mapping: SourceVolumeMapping) -> [u8; 512] {
    let mut mbr = [0; 512];
    // Boot code is deliberately absent until WinPE performs bootsect/bcdboot.
    // This partition map is a generated disk layout, never an original MBR.
    mbr[440..444].copy_from_slice(&mapping.disk_signature.to_le_bytes());
    mbr[446] = 0x80;
    mbr[447..450].copy_from_slice(&[0xfe, 0xff, 0xff]);
    mbr[450] = 0x07;
    mbr[451..454].copy_from_slice(&[0xfe, 0xff, 0xff]);
    mbr[454..458].copy_from_slice(&((mapping.guest_volume_offset / 512) as u32).to_le_bytes());
    mbr[458..462].copy_from_slice(&((mapping.source_volume_length / 512) as u32).to_le_bytes());
    mbr[510..512].copy_from_slice(&[0x55, 0xaa]);
    mbr
}

impl BlockProvider for NtfsBootDisk {
    fn len(&self) -> u64 {
        self.length
    }
    fn read_exact_at(&self, offset: u64, buffer: &mut [u8]) -> Result<(), BlockDeviceError> {
        let end = offset
            .checked_add(buffer.len() as u64)
            .ok_or(BlockDeviceError::ArithmeticOverflow)?;
        if end > self.length {
            return Err(BlockDeviceError::OutOfBounds {
                offset,
                end,
                size: self.length,
            });
        }
        buffer.fill(0);
        if offset < 512 {
            let count = (512 - offset).min(buffer.len() as u64) as usize;
            buffer[..count].copy_from_slice(&self.mbr[offset as usize..offset as usize + count]);
        }
        let start = offset.max(self.mapping.guest_volume_offset);
        if end > start {
            let begin = (start - offset) as usize;
            self.parent.read_exact_at(
                start - self.mapping.guest_volume_offset,
                &mut buffer[begin..],
            )?;
        }
        Ok(())
    }
}
