//! exFAT Boot Sector parsing.
//!
//! The Boot Sector is the first sector of an exFAT volume and contains
//! essential parameters for mounting and navigating the file system.

use crate::types::*;
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
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "boot sector too small (need 512 bytes)",
            ));
        }

        // Validate JumpBoot
        if data[0..3] != JUMP_BOOT {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "invalid jump boot: expected {:02X?}, got {:02X?}",
                    JUMP_BOOT,
                    &data[0..3]
                ),
            ));
        }

        // Validate FileSystemName
        if &data[3..11] != EXFAT_MAGIC {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "not an exFAT volume (invalid file system name)",
            ));
        }

        // Validate MustBeZero field (bytes 11-63)
        if data[11..64].iter().any(|&b| b != 0) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "non-zero bytes in MustBeZero field (possible FAT12/16/32 volume)",
            ));
        }

        // Validate BootSignature
        let signature = u16::from_le_bytes([data[510], data[511]]);
        if signature != BOOT_SIGNATURE {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "invalid boot signature: expected {:04X}, got {:04X}",
                    BOOT_SIGNATURE, signature
                ),
            ));
        }

        // Parse fields
        let bytes_per_sector_shift = data[108];
        let sectors_per_cluster_shift = data[109];

        // Validate shift values
        if !(9..=12).contains(&bytes_per_sector_shift) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "invalid BytesPerSectorShift: {} (must be 9-12)",
                    bytes_per_sector_shift
                ),
            ));
        }

        if sectors_per_cluster_shift > 25 - bytes_per_sector_shift {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "invalid SectorsPerClusterShift: {} (max for {} byte sectors is {})",
                    sectors_per_cluster_shift,
                    1 << bytes_per_sector_shift,
                    25 - bytes_per_sector_shift
                ),
            ));
        }

        let number_of_fats = data[110];
        if number_of_fats != 1 && number_of_fats != 2 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("invalid NumberOfFats: {} (must be 1 or 2)", number_of_fats),
            ));
        }

        // Helper closure to safely extract fixed-size arrays
        // SAFETY: We already validated data.len() >= 512 at the top of parse()
        let file_system_name: [u8; 8] = data[3..11]
            .try_into()
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid filesystem name slice"))?;

        Ok(Self {
            jump_boot: [data[0], data[1], data[2]],
            file_system_name,
            partition_offset: u64::from_le_bytes(
                data[64..72].try_into().map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid partition offset"))?
            ),
            volume_length: u64::from_le_bytes(
                data[72..80].try_into().map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid volume length"))?
            ),
            fat_offset: u32::from_le_bytes(
                data[80..84].try_into().map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid fat offset"))?
            ),
            fat_length: u32::from_le_bytes(
                data[84..88].try_into().map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid fat length"))?
            ),
            cluster_heap_offset: u32::from_le_bytes(
                data[88..92].try_into().map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid cluster heap offset"))?
            ),
            cluster_count: u32::from_le_bytes(
                data[92..96].try_into().map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid cluster count"))?
            ),
            first_cluster_of_root: u32::from_le_bytes(
                data[96..100].try_into().map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid first cluster of root"))?
            ),
            volume_serial_number: u32::from_le_bytes(
                data[100..104].try_into().map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid volume serial"))?
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
        self.cluster_heap_byte_offset()
            + (cluster - MIN_CLUSTER) as u64 * self.cluster_size()
    }

    /// Get the major revision number.
    pub fn revision_major(&self) -> u8 {
        (self.file_system_revision >> 8) as u8
    }

    /// Get the minor revision number.
    pub fn revision_minor(&self) -> u8 {
        (self.file_system_revision & 0xFF) as u8
    }

    /// Check if the ActiveFat flag indicates the second FAT is active.
    pub fn is_second_fat_active(&self) -> bool {
        self.volume_flags & 0x0001 != 0
    }

    /// Check if the volume dirty flag is set.
    pub fn is_volume_dirty(&self) -> bool {
        self.volume_flags & 0x0002 != 0
    }

    /// Check if media failure has been reported.
    pub fn has_media_failure(&self) -> bool {
        self.volume_flags & 0x0004 != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_valid_boot_sector() -> Vec<u8> {
        let mut data = vec![0u8; 512];
        data[0..3].copy_from_slice(&JUMP_BOOT);
        data[3..11].copy_from_slice(EXFAT_MAGIC);
        // PartitionOffset = 0
        // VolumeLength = 1024 sectors (512KB)
        data[72..80].copy_from_slice(&1024u64.to_le_bytes());
        // FatOffset = 24
        data[80..84].copy_from_slice(&24u32.to_le_bytes());
        // FatLength = 1
        data[84..88].copy_from_slice(&1u32.to_le_bytes());
        // ClusterHeapOffset = 32
        data[88..92].copy_from_slice(&32u32.to_le_bytes());
        // ClusterCount = 100
        data[92..96].copy_from_slice(&100u32.to_le_bytes());
        // FirstClusterOfRootDirectory = 5
        data[96..100].copy_from_slice(&5u32.to_le_bytes());
        // VolumeSerialNumber = 0x12345678
        data[100..104].copy_from_slice(&0x12345678u32.to_le_bytes());
        // FileSystemRevision = 1.00
        data[104..106].copy_from_slice(&0x0100u16.to_le_bytes());
        // VolumeFlags = 0
        data[106..108].copy_from_slice(&0u16.to_le_bytes());
        // BytesPerSectorShift = 9 (512 bytes)
        data[108] = 9;
        // SectorsPerClusterShift = 1 (2 sectors per cluster)
        data[109] = 1;
        // NumberOfFats = 1
        data[110] = 1;
        // DriveSelect = 0x80
        data[111] = 0x80;
        // PercentInUse = 0xFF (unknown)
        data[112] = 0xFF;
        // BootSignature
        data[510..512].copy_from_slice(&BOOT_SIGNATURE.to_le_bytes());
        data
    }

    #[test]
    fn parse_valid_boot_sector() {
        let data = make_valid_boot_sector();
        let boot = ExfatBootSector::parse(&data).unwrap();

        assert_eq!(boot.bytes_per_sector(), 512);
        assert_eq!(boot.sectors_per_cluster(), 2);
        assert_eq!(boot.cluster_size(), 1024);
        assert_eq!(boot.fat_offset, 24);
        assert_eq!(boot.fat_length, 1);
        assert_eq!(boot.cluster_heap_offset, 32);
        assert_eq!(boot.cluster_count, 100);
        assert_eq!(boot.first_cluster_of_root, 5);
        assert_eq!(boot.volume_serial_number, 0x12345678);
        assert_eq!(boot.revision_major(), 1);
        assert_eq!(boot.revision_minor(), 0);
        assert_eq!(boot.number_of_fats, 1);
    }

    #[test]
    fn reject_invalid_jump_boot() {
        let mut data = make_valid_boot_sector();
        data[0] = 0x90; // Invalid
        assert!(ExfatBootSector::parse(&data).is_err());
    }

    #[test]
    fn reject_invalid_magic() {
        let mut data = make_valid_boot_sector();
        data[3..11].copy_from_slice(b"FAT32   ");
        assert!(ExfatBootSector::parse(&data).is_err());
    }

    #[test]
    fn reject_invalid_signature() {
        let mut data = make_valid_boot_sector();
        data[510..512].copy_from_slice(&0x0000u16.to_le_bytes());
        assert!(ExfatBootSector::parse(&data).is_err());
    }

    #[test]
    fn reject_nonzero_mustbezero() {
        let mut data = make_valid_boot_sector();
        data[20] = 0x01; // Non-zero in MustBeZero field
        assert!(ExfatBootSector::parse(&data).is_err());
    }

    #[test]
    fn cluster_to_offset_calculation() {
        let data = make_valid_boot_sector();
        let boot = ExfatBootSector::parse(&data).unwrap();

        // Cluster 2 should be at cluster_heap_offset * bytes_per_sector
        assert_eq!(boot.cluster_to_offset(2), 32 * 512);
        // Cluster 3 should be one cluster later
        assert_eq!(boot.cluster_to_offset(3), 32 * 512 + 1024);
    }

    #[test]
    fn too_small_data_rejected() {
        let data = vec![0u8; 100];
        assert!(ExfatBootSector::parse(&data).is_err());
    }
}
