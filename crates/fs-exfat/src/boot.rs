//! exFAT Boot Sector parsing.
//!
//! The Boot Sector is the first sector of an exFAT volume and contains
//! essential parameters for mounting and navigating the file system.

use crate::types::*;
use evidence_core::filesystem::invalid_fs_data;
use std::io;

/// Parsed exFAT Boot Sector.
#[derive(Debug, Clone)]
pub struct ExfatBootSector {
    /// Jump instruction (should be EBh 76h 90h)
    pub jump_boot: [u8; 3],
    /// File system name (should be "EXFAT   ")
    pub file_system_name: [u8; 8],
    /// Sector offset of the partition
    pub partition_offset: u64,
    /// Volume size in sectors
    pub volume_length: u64,
    /// Sector offset of the first FAT
    pub fat_offset: u32,
    /// Size of each FAT in sectors
    pub fat_length: u32,
    /// Sector offset of the Cluster Heap
    pub cluster_heap_offset: u32,
    /// Number of clusters in the Cluster Heap
    pub cluster_count: u32,
    /// First cluster of the root directory
    pub first_cluster_of_root: u32,
    /// Volume serial number
    pub volume_serial_number: u32,
    /// File system revision (major.minor)
    pub file_system_revision: u16,
    /// Volume flags
    pub volume_flags: u16,
    /// Bytes per sector as log2(N)
    pub bytes_per_sector_shift: u8,
    /// Sectors per cluster as log2(N)
    pub sectors_per_cluster_shift: u8,
    /// Number of FATs (1 or 2)
    pub number_of_fats: u8,
    /// Drive number for INT 13h
    pub drive_select: u8,
    /// Percentage of allocated clusters (0-100 or 0xFF if unknown)
    pub percent_in_use: u8,
}

impl ExfatBootSector {
    /// Parse a 512-byte boot sector.
    ///
    /// Validates magic bytes and signature before parsing.
    pub fn parse(data: &[u8]) -> io::Result<Self> {
        if data.len() < 512 {
            return Err(invalid_fs_data("boot sector too small (need 512 bytes)"));
        }

        // Validate JumpBoot
        if data[0..3] != JUMP_BOOT {
            return Err(invalid_fs_data(format!(
                "invalid jump boot: expected {:02X?}, got {:02X?}",
                JUMP_BOOT,
                &data[0..3]
            )));
        }

        // Validate FileSystemName
        if &data[3..11] != EXFAT_MAGIC {
            return Err(invalid_fs_data(
                "not an exFAT volume (invalid file system name)",
            ));
        }

        // Validate MustBeZero field (bytes 11-63)
        if data[11..64].iter().any(|&b| b != 0) {
            return Err(invalid_fs_data(
                "non-zero bytes in MustBeZero field (possible FAT12/16/32 volume)",
            ));
        }

        // Validate BootSignature
        let signature = u16::from_le_bytes([data[510], data[511]]);
        if signature != BOOT_SIGNATURE {
            return Err(invalid_fs_data(format!(
                "invalid boot signature: expected {:04X}, got {:04X}",
                BOOT_SIGNATURE, signature
            )));
        }

        // Parse fields
        let bytes_per_sector_shift = data[108];
        let sectors_per_cluster_shift = data[109];

        // Validate shift values
        if !(9..=12).contains(&bytes_per_sector_shift) {
            return Err(invalid_fs_data(format!(
                "invalid BytesPerSectorShift: {} (must be 9-12)",
                bytes_per_sector_shift
            )));
        }

        if sectors_per_cluster_shift > 25 - bytes_per_sector_shift {
            return Err(invalid_fs_data(format!(
                "invalid SectorsPerClusterShift: {} (max for {} byte sectors is {})",
                sectors_per_cluster_shift,
                1 << bytes_per_sector_shift,
                25 - bytes_per_sector_shift
            )));
        }

        let number_of_fats = data[110];
        if number_of_fats != 1 && number_of_fats != 2 {
            return Err(invalid_fs_data(format!(
                "invalid NumberOfFats: {} (must be 1 or 2)",
                number_of_fats
            )));
        }

        // Helper closure to safely extract fixed-size arrays
        // SAFETY: We already validated data.len() >= 512 at the top of parse()
        let file_system_name: [u8; 8] = data[3..11]
            .try_into()
            .map_err(|_| invalid_fs_data("invalid filesystem name slice"))?;

        Ok(Self {
            jump_boot: [data[0], data[1], data[2]],
            file_system_name,
            partition_offset: u64::from_le_bytes(
                data[64..72]
                    .try_into()
                    .map_err(|_| invalid_fs_data("invalid partition offset"))?,
            ),
            volume_length: u64::from_le_bytes(
                data[72..80]
                    .try_into()
                    .map_err(|_| invalid_fs_data("invalid volume length"))?,
            ),
            fat_offset: u32::from_le_bytes(
                data[80..84]
                    .try_into()
                    .map_err(|_| invalid_fs_data("invalid fat offset"))?,
            ),
            fat_length: u32::from_le_bytes(
                data[84..88]
                    .try_into()
                    .map_err(|_| invalid_fs_data("invalid fat length"))?,
            ),
            cluster_heap_offset: u32::from_le_bytes(
                data[88..92]
                    .try_into()
                    .map_err(|_| invalid_fs_data("invalid cluster heap offset"))?,
            ),
            cluster_count: u32::from_le_bytes(
                data[92..96]
                    .try_into()
                    .map_err(|_| invalid_fs_data("invalid cluster count"))?,
            ),
            first_cluster_of_root: u32::from_le_bytes(
                data[96..100]
                    .try_into()
                    .map_err(|_| invalid_fs_data("invalid first cluster of root"))?,
            ),
            volume_serial_number: u32::from_le_bytes(
                data[100..104]
                    .try_into()
                    .map_err(|_| invalid_fs_data("invalid volume serial"))?,
            ),
            file_system_revision: u16::from_le_bytes([data[104], data[105]]),
            volume_flags: u16::from_le_bytes([data[106], data[107]]),
            bytes_per_sector_shift,
            sectors_per_cluster_shift,
            number_of_fats,
            drive_select: data[111],
            percent_in_use: data[112],
        })
    }

    /// Bytes per sector (typically 512).
    pub fn bytes_per_sector(&self) -> u32 {
        1u32 << self.bytes_per_sector_shift
    }

    /// Sectors per cluster.
    pub fn sectors_per_cluster(&self) -> u32 {
        1u32 << self.sectors_per_cluster_shift
    }

    /// Cluster size in bytes.
    pub fn cluster_size(&self) -> u64 {
        self.bytes_per_sector() as u64 * self.sectors_per_cluster() as u64
    }

    /// Byte offset of the FAT region.
    pub fn fat_byte_offset(&self) -> u64 {
        self.fat_offset as u64 * self.bytes_per_sector() as u64
    }

    /// Byte offset of the Cluster Heap.
    pub fn cluster_heap_byte_offset(&self) -> u64 {
        self.cluster_heap_offset as u64 * self.bytes_per_sector() as u64
    }

    /// Convert a cluster index to a byte offset in the volume.
    ///
    /// Cluster 2 is the first data cluster.
    pub fn cluster_to_offset(&self, cluster: u32) -> u64 {
        if cluster < MIN_CLUSTER {
            return 0;
        }
        self.cluster_heap_byte_offset() + (cluster - MIN_CLUSTER) as u64 * self.cluster_size()
    }

    /// Get the major revision number.
    pub fn revision_major(&self) -> u8 {
        (self.file_system_revision >> 8) as u8
    }

    /// Get the minor revision number.
    pub fn revision_minor(&self) -> u8 {
        (self.file_system_revision & 0xFF) as u8
    }
}

#[cfg(test)]
#[path = "../tests/unit/boot.rs"]
mod tests;
