use evidence_core::volume::mbr;

#[test]
fn parse_partition_table() {
    let mut data = vec![0u8; 512];
    data[0x1FE] = 0x55;
    data[0x1FF] = 0xAA;

    // Entry 0: NTFS partition at LBA 2048, 100K sectors
    let e0 = 446;
    data[e0] = 0x80;
    data[e0 + 4] = 0x07; // NTFS
    data[e0 + 8..e0 + 12].copy_from_slice(&2048u32.to_le_bytes());
    data[e0 + 12..e0 + 16].copy_from_slice(&100000u32.to_le_bytes());

    // Entry 1: empty
    // Entry 2: FAT32 at LBA 102048
    let e2 = 446 + 2 * 16;
    data[e2 + 4] = 0x0B; // FAT32
    data[e2 + 8..e2 + 12].copy_from_slice(&102048u32.to_le_bytes());
    data[e2 + 12..e2 + 16].copy_from_slice(&50000u32.to_le_bytes());

    let entries = mbr::parse_partition_table(&data);
    assert_eq!(entries.len(), 4);
    assert_eq!(entries[0].partition_type, 0x07);
    assert_eq!(entries[0].lba_start, 2048);
    assert_eq!(entries[2].partition_type, 0x0B);
}

#[test]
fn find_first_ntfs() {
    let entries = vec![
        mbr::PartitionEntry {
            partition_number: 0,
            is_logical: false,
            bootable: false,
            partition_type: 0x0B,
            lba_start: 100,
            sector_count: 500,
            ebr_lba: None,
        },
        mbr::PartitionEntry {
            partition_number: 1,
            is_logical: false,
            bootable: true,
            partition_type: 0x07,
            lba_start: 2048,
            sector_count: 100000,
            ebr_lba: None,
        },
        mbr::PartitionEntry {
            partition_number: 2,
            is_logical: false,
            bootable: false,
            partition_type: 0x07,
            lba_start: 0,
            sector_count: 0,
            ebr_lba: None,
        },
    ];
    let ntfs = mbr::find_first_ntfs(&entries).unwrap();
    assert_eq!(ntfs.lba_start, 2048);
}

#[test]
fn empty_mbr_returns_empty() {
    let data = [0u8; 64];
    let entries = mbr::parse_partition_table(&data);
    assert_eq!(entries.len(), 0);
}
