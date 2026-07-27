use super::*;

#[test]
fn parses_four_primary_partitions() {
    let mut mbr = vec![0u8; 512];
    mbr[510] = 0x55;
    mbr[511] = 0xAA;
    // Entry 0: NTFS at LBA 2048, 100000 sectors
    let base0 = 446;
    mbr[base0] = 0x80; // bootable
    mbr[base0 + 4] = 0x07; // NTFS
    mbr[base0 + 8..base0 + 12].copy_from_slice(&2048u32.to_le_bytes());
    mbr[base0 + 12..base0 + 16].copy_from_slice(&100000u32.to_le_bytes());
    // Entry 1: empty
    // Entry 2: extended (0x0F) at LBA 200000
    let base2 = 446 + 2 * 16;
    mbr[base2 + 4] = 0x0F;
    mbr[base2 + 8..base2 + 12].copy_from_slice(&200000u32.to_le_bytes());
    mbr[base2 + 12..base2 + 16].copy_from_slice(&50000u32.to_le_bytes());

    let entries = parse_partition_table(&mbr);
    assert_eq!(entries.len(), 4);
    assert_eq!(entries[0].partition_number, 0);
    assert!(!entries[0].is_logical);
    assert!(entries[0].bootable);
    assert_eq!(entries[0].partition_type, 0x07);
    assert_eq!(entries[0].lba_start, 2048);
    assert!(entries[2].is_extended());
    assert_eq!(entries[2].ebr_lba, Some(200000));
}

#[test]
fn parses_ebr_chain() {
    use std::io::Cursor;
    let sectors = 220_066; // EBR at LBA 220063 + 2 sectors padding
    let mut disk = vec![0u8; 512 * sectors];

    // MBR at sector 0
    disk[510] = 0x55;
    disk[511] = 0xAA;
    let base0 = 446;
    disk[base0 + 4] = 0x07; // NTFS primary
    disk[base0 + 8..base0 + 12].copy_from_slice(&2048u32.to_le_bytes());
    disk[base0 + 12..base0 + 16].copy_from_slice(&10000u32.to_le_bytes());
    // Entry 1: extended at LBA 200000
    let base1 = 446 + 16;
    disk[base1 + 4] = 0x0F;
    disk[base1 + 8..base1 + 12].copy_from_slice(&200000u32.to_le_bytes());
    disk[base1 + 12..base1 + 16].copy_from_slice(&50000u32.to_le_bytes());

    // EBR at sector 200000: logical volume at relative LBA 63
    let ebr1_off = 200000 * 512;
    disk[ebr1_off + 510] = 0x55;
    disk[ebr1_off + 511] = 0xAA;
    disk[ebr1_off + 446 + 4] = 0x07; // NTFS logical
    disk[ebr1_off + 446 + 8..ebr1_off + 446 + 12].copy_from_slice(&63u32.to_le_bytes());
    disk[ebr1_off + 446 + 12..ebr1_off + 446 + 16].copy_from_slice(&20000u32.to_le_bytes());
    // Second entry: next EBR at relative LBA 20063
    disk[ebr1_off + 462 + 4] = 0x05;
    disk[ebr1_off + 462 + 8..ebr1_off + 462 + 12].copy_from_slice(&20063u32.to_le_bytes());

    // EBR at sector 200000+20063 = 220063: logical volume at relative LBA 63
    let ebr2_off = (200000 + 20063) * 512;
    disk[ebr2_off + 510] = 0x55;
    disk[ebr2_off + 511] = 0xAA;
    disk[ebr2_off + 446 + 4] = 0x07;
    disk[ebr2_off + 446 + 8..ebr2_off + 446 + 12].copy_from_slice(&63u32.to_le_bytes());
    disk[ebr2_off + 446 + 12..ebr2_off + 446 + 16].copy_from_slice(&10000u32.to_le_bytes());
    // Second entry: zeroed (end of chain)
    // (already zeros)

    let mut cursor = Cursor::new(&disk);
    let entries = parse_mbr_full(&mut cursor).unwrap();

    assert_eq!(
        entries.len(),
        4,
        "1 primary NTFS + 1 extended + 2 logical = 4 entries"
    );
    // Entry 0: primary NTFS
    assert_eq!(entries[0].partition_number, 0);
    assert!(!entries[0].is_logical);
    assert_eq!(entries[0].lba_start, 2048);
    // Entry 1: extended (not a data partition)
    assert!(entries[1].is_extended());
    // Entry 2: first logical
    assert_eq!(entries[2].partition_number, 2);
    assert!(entries[2].is_logical);
    assert_eq!(entries[2].lba_start, 200000 + 63);
    assert_eq!(entries[2].sector_count, 20000);
    // Entry 3: second logical
    assert_eq!(entries[3].partition_number, 3);
    assert!(entries[3].is_logical);
    assert_eq!(entries[3].lba_start, 200000 + 20063 + 63);
}

#[test]
fn classify_mbr_partition_type_known_types() {
    // Supported types
    assert_eq!(classify_mbr_partition_type(0x01).name, "FAT12");
    assert_eq!(
        classify_mbr_partition_type(0x01).status,
        MbrPartitionStatus::Supported
    );
    assert_eq!(classify_mbr_partition_type(0x07).name, "NTFS/exFAT/HPFS");
    assert_eq!(
        classify_mbr_partition_type(0x07).status,
        MbrPartitionStatus::Supported
    );
    assert_eq!(classify_mbr_partition_type(0x0C).name, "FAT32 (LBA)");
    assert_eq!(
        classify_mbr_partition_type(0x0C).status,
        MbrPartitionStatus::Supported
    );

    // 0x42 is LDM, not BitLocker.
    assert_eq!(
        classify_mbr_partition_type(0x42).name,
        "Windows dynamic disk (LDM)"
    );
    assert_eq!(
        classify_mbr_partition_type(0x42).status,
        MbrPartitionStatus::Unsupported
    );

    // Unsupported Linux
    assert_eq!(classify_mbr_partition_type(0x83).name, "Linux");
    assert_eq!(
        classify_mbr_partition_type(0x83).status,
        MbrPartitionStatus::Unsupported
    );

    // Extended
    assert_eq!(classify_mbr_partition_type(0x05).name, "Extended");
    assert_eq!(classify_mbr_partition_type(0x0F).name, "Extended");

    // Empty
    assert_eq!(classify_mbr_partition_type(0x00).name, "Empty");

    // Unknown
    assert_eq!(classify_mbr_partition_type(0xFE).name, "Unknown");
}

#[test]
fn no_mbr_type_byte_reports_bitlocker() {
    // MBR has no BitLocker partition type. Windows leaves the original type byte
    // (normally 0x07) in place on an encrypted MBR volume, so claiming any byte
    // means BitLocker would both mislabel real volumes (0x42 = dynamic disk) and
    // let a caller skip the `-FVE-FS-` boot-sector check that actually detects it.
    for byte in 0u8..=0xFF {
        assert_ne!(
            classify_mbr_partition_type(byte).status,
            MbrPartitionStatus::EncryptedBitLocker,
            "MBR type byte {byte:#04X} must not classify as BitLocker"
        );
    }
}

#[test]
fn parse_mbr_full_excludes_empty_and_extended_from_partition_records() {
    use std::io::Cursor;
    // Build a disk with: 1 NTFS primary, 1 empty, 1 extended (with 1 logical)
    let mut disk = vec![0u8; 512 * 1000];
    disk[510] = 0x55;
    disk[511] = 0xAA;

    // Entry 0: NTFS at LBA 63, 500 sectors
    let base0 = 446;
    disk[base0] = 0x80;
    disk[base0 + 4] = 0x07;
    disk[base0 + 8..base0 + 12].copy_from_slice(&63u32.to_le_bytes());
    disk[base0 + 12..base0 + 16].copy_from_slice(&500u32.to_le_bytes());

    // Entry 1: empty (type 0x00) — should be filtered out
    // (already zeroed)

    // Entry 2: extended at LBA 600
    let base2 = 446 + 2 * 16;
    disk[base2 + 4] = 0x0F;
    disk[base2 + 8..base2 + 12].copy_from_slice(&600u32.to_le_bytes());
    disk[base2 + 12..base2 + 16].copy_from_slice(&300u32.to_le_bytes());

    // EBR at sector 600: logical volume at relative LBA 63
    let ebr_off = 600 * 512;
    disk[ebr_off + 510] = 0x55;
    disk[ebr_off + 511] = 0xAA;
    disk[ebr_off + 446 + 4] = 0x07; // NTFS logical
    disk[ebr_off + 446 + 8..ebr_off + 446 + 12].copy_from_slice(&63u32.to_le_bytes());
    disk[ebr_off + 446 + 12..ebr_off + 446 + 16].copy_from_slice(&200u32.to_le_bytes());

    let mut cursor = Cursor::new(&disk);
    let entries = parse_mbr_full(&mut cursor).unwrap();

    // Should have 3 entries: NTFS primary + extended + 1 logical
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0].partition_type, 0x07);
    assert!(entries[1].is_extended());
    assert!(entries[2].is_logical);
    assert_eq!(entries[2].partition_type, 0x07);

    // The non-empty, non-extended entries suitable for PartitionRecord: 2 data partitions
    let data_partitions: Vec<_> = entries
        .iter()
        .filter(|e| !e.is_extended() && e.partition_type != 0)
        .collect();
    assert_eq!(data_partitions.len(), 2);
    assert_eq!(data_partitions[0].partition_type, 0x07);
    assert!(!data_partitions[0].is_logical);
    assert_eq!(data_partitions[1].partition_type, 0x07);
    assert!(data_partitions[1].is_logical);

    // Verify the classification maps correctly
    let class = classify_mbr_partition_type(data_partitions[0].partition_type);
    assert_eq!(class.name, "NTFS/exFAT/HPFS");
    assert_eq!(class.status, MbrPartitionStatus::Supported);
}

#[test]
fn all_ntfs_returns_primary_and_logical() {
    let entries = vec![
        PartitionEntry {
            partition_number: 0,
            is_logical: false,
            bootable: true,
            partition_type: 0x07,
            lba_start: 2048,
            sector_count: 1000,
            ebr_lba: None,
        },
        PartitionEntry {
            partition_number: 1,
            is_logical: true,
            bootable: false,
            partition_type: 0x07,
            lba_start: 200063,
            sector_count: 2000,
            ebr_lba: Some(200000),
        },
        PartitionEntry {
            partition_number: 2,
            is_logical: false,
            bootable: false,
            partition_type: 0x83,
            lba_start: 400000,
            sector_count: 5000,
            ebr_lba: None,
        },
    ];
    let ntfs = all_ntfs(&entries);
    assert_eq!(ntfs.len(), 2);
    assert_eq!(ntfs[0].partition_number, 0);
    assert!(ntfs[1].is_logical);
}
