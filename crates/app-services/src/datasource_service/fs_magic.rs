use super::{ImageFilesystemKind, Result};
use std::io::{Read, Seek, SeekFrom};

pub(crate) const SECTOR_SIZE: u64 = 512;

pub(crate) fn kind_label(kind: ImageFilesystemKind) -> String {
    match kind {
        ImageFilesystemKind::Ntfs => "NTFS".to_string(),
        ImageFilesystemKind::Fat => "FAT".to_string(),
        ImageFilesystemKind::BitLocker => "BitLocker".to_string(),
        ImageFilesystemKind::Ext4 => "Ext4".to_string(),
        ImageFilesystemKind::Xfs => "XFS".to_string(),
        ImageFilesystemKind::Btrfs => "Btrfs".to_string(),
        ImageFilesystemKind::LvmPool => "LVM".to_string(),
    }
}

pub(crate) fn detect_boot_filesystem(sector: &[u8; 512]) -> Option<ImageFilesystemKind> {
    if &sector[3..11] == b"NTFS    " {
        return Some(ImageFilesystemKind::Ntfs);
    }

    if looks_like_bitlocker_boot_sector(sector) {
        return Some(ImageFilesystemKind::BitLocker);
    }

    if looks_like_fat_boot_sector(sector) {
        return Some(ImageFilesystemKind::Fat);
    }

    None
}

pub(crate) fn read_boot_filesystem<R>(
    reader: &mut R,
    offset: u64,
) -> Result<Option<ImageFilesystemKind>>
where
    R: Read + Seek + ?Sized,
{
    let sector = read_sector(reader, offset)?;
    // LVM PV labels are authoritative when present. Probe them before
    // ordinary filesystem magics so stale signatures inside a PV do not bypass
    // LV expansion.
    match fs_lvm::probe_lvm(reader, offset) {
        Ok(true) => return Ok(Some(ImageFilesystemKind::LvmPool)),
        Ok(false) => {}
        Err(_e) => {
            tracing::debug!("LVM probe error at offset {}: {}", offset, _e);
        }
    }

    if let Some(kind) = detect_boot_filesystem(&sector) {
        return Ok(Some(kind));
    }

    // Check for XFS at sector 0 (big-endian magic "XFSB")
    if offset.is_multiple_of(512) {
        let magic = u32::from_be_bytes([sector[0], sector[1], sector[2], sector[3]]);
        if magic == 0x5846_5342 {
            return Ok(Some(ImageFilesystemKind::Xfs));
        }
    }

    // Check for ext4 superblock magic at byte 0x38 within the superblock.
    reader.seek(SeekFrom::Start(offset + 1024 + 0x38))?;
    let mut sb = [0u8; 2];
    if reader.read_exact(&mut sb).is_ok() && u16::from_le_bytes(sb) == 0xEF53 {
        return Ok(Some(ImageFilesystemKind::Ext4));
    }

    // Check for Btrfs magic at byte 0x40 within the primary superblock.
    reader.seek(SeekFrom::Start(offset + 0x10000 + 0x40))?;
    let mut btrfs_magic = [0u8; 8];
    if reader.read_exact(&mut btrfs_magic).is_ok() && &btrfs_magic == b"_BHRfS_M" {
        return Ok(Some(ImageFilesystemKind::Btrfs));
    }

    Ok(None)
}

pub(crate) fn read_sector<R>(reader: &mut R, offset: u64) -> Result<[u8; 512]>
where
    R: Read + Seek + ?Sized,
{
    let mut sector = [0u8; 512];
    reader.seek(SeekFrom::Start(offset))?;
    reader.read_exact(&mut sector)?;
    Ok(sector)
}

fn looks_like_bitlocker_boot_sector(sector: &[u8; 512]) -> bool {
    &sector[3..11] == b"-FVE-FS-"
}

fn looks_like_fat_boot_sector(sector: &[u8; 512]) -> bool {
    if sector[510] != 0x55 || sector[511] != 0xAA {
        return false;
    }

    let bytes_per_sector = u16::from_le_bytes([sector[11], sector[12]]);
    if !matches!(bytes_per_sector, 512 | 1024 | 2048 | 4096) {
        return false;
    }

    let sectors_per_cluster = sector[13];
    if sectors_per_cluster == 0 || !sectors_per_cluster.is_power_of_two() {
        return false;
    }

    let reserved_sectors = u16::from_le_bytes([sector[14], sector[15]]);
    let fat_count = sector[16];
    if reserved_sectors == 0 || fat_count == 0 || fat_count > 2 {
        return false;
    }

    let fat16_label = &sector[54..62];
    let fat32_label = &sector[82..90];
    if matches!(fat16_label, b"FAT12   " | b"FAT16   ") || fat32_label == b"FAT32   " {
        return true;
    }

    let total16 = u16::from_le_bytes([sector[19], sector[20]]);
    let total32 = u32::from_le_bytes(sector[32..36].try_into().unwrap_or([0; 4]));
    let fat16_sectors = u16::from_le_bytes([sector[22], sector[23]]);
    let fat32_sectors = u32::from_le_bytes(sector[36..40].try_into().unwrap_or([0; 4]));

    (total16 != 0 || total32 != 0) && (fat16_sectors != 0 || fat32_sectors != 0)
}
