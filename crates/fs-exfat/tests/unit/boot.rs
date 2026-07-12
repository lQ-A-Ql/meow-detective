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
