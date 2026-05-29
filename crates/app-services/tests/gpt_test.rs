use evidence_core::volume::gpt;

#[test]
fn parse_header_valid() {
    let mut data = vec![0u8; 512];
    data[0..8].copy_from_slice(b"EFI PART");
    data[12..16].copy_from_slice(&92u32.to_le_bytes());
    data[40..48].copy_from_slice(&34u64.to_le_bytes());
    let last = (1024u64 * 1024).to_le_bytes();
    data[48..56].copy_from_slice(&last);
    data[72..80].copy_from_slice(&2u64.to_le_bytes());
    data[80..84].copy_from_slice(&128u32.to_le_bytes());
    data[84..88].copy_from_slice(&128u32.to_le_bytes());

    let hdr = gpt::parse_gpt_header(&data).unwrap();
    assert_eq!(hdr.first_usable_lba, 34);
    assert_eq!(hdr.partition_count, 128);
}

#[test]
fn parse_entries_finds_partition() {
    let count = 4u32;
    let esz = 128u32;
    let mut data = vec![0u8; (count * esz) as usize];

    let e1 = 128;
    // MS Basic Data GUID
    data[e1..e1 + 16].copy_from_slice(&[
        0xA2, 0xA0, 0xD0, 0xEB, 0xE5, 0xB9, 0x33, 0x44, 0x87, 0xC0, 0x68, 0xB6, 0xB7, 0x26, 0x99,
        0xC7,
    ]);
    data[e1 + 32..e1 + 40].copy_from_slice(&2048u64.to_le_bytes());
    data[e1 + 40..e1 + 48].copy_from_slice(&100000u64.to_le_bytes());
    for (i, c) in "Windows".encode_utf16().enumerate() {
        data[e1 + 56 + i * 2..e1 + 56 + i * 2 + 2].copy_from_slice(&c.to_le_bytes());
    }

    let parts = gpt::parse_gpt_entries(&data, esz, count);
    assert_eq!(parts.len(), 1);
    assert_eq!(parts[0].start_lba, 2048);
    assert_eq!(parts[0].index, 2);

    let found = gpt::find_first_data_partition(&parts);
    assert!(found.is_some());
}

#[test]
fn classify_known_partition_types() {
    let efi: [u8; 16] = [
        0x28, 0x73, 0x2A, 0xC1, 0x1F, 0xF8, 0xD2, 0x11, 0xBA, 0x4B, 0x00, 0xA0, 0xC9, 0x3E, 0xC9,
        0x3B,
    ];
    let msr: [u8; 16] = [
        0x16, 0xE3, 0xC9, 0xE3, 0x5C, 0x0B, 0xB8, 0x4D, 0x81, 0x7D, 0xF9, 0x2D, 0xF0, 0x02, 0x15,
        0xAE,
    ];
    let recovery: [u8; 16] = [
        0xA4, 0xBB, 0x94, 0xDE, 0xD1, 0x06, 0x40, 0x4D, 0xA1, 0x6A, 0xBF, 0xD5, 0x01, 0x79, 0xD6,
        0xAC,
    ];

    assert_eq!(
        gpt::classify_partition_type(&efi),
        gpt::GptPartitionType::EfiSystem
    );
    assert_eq!(
        gpt::classify_partition_type(&msr),
        gpt::GptPartitionType::MicrosoftReserved
    );
    assert_eq!(
        gpt::classify_partition_type(&recovery),
        gpt::GptPartitionType::WindowsRecovery
    );
}

#[test]
fn detect_image_filesystem_returns_multiple_gpt_candidates() {
    use app_services::datasource_service::{
        detect_image_filesystem, ImageFilesystemKind, ImageFilesystemSource, PartitionStatus,
    };
    use std::io::Cursor;

    let mut image = vec![0u8; 4096 * 512];

    image[510] = 0x55;
    image[511] = 0xAA;
    image[446 + 4] = 0xEE;
    image[446 + 8..446 + 12].copy_from_slice(&1u32.to_le_bytes());
    image[446 + 12..446 + 16].copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes());

    let gpt_header = &mut image[512..1024];
    gpt_header[0..8].copy_from_slice(b"EFI PART");
    gpt_header[12..16].copy_from_slice(&92u32.to_le_bytes());
    gpt_header[72..80].copy_from_slice(&2u64.to_le_bytes());
    gpt_header[80..84].copy_from_slice(&4u32.to_le_bytes());
    gpt_header[84..88].copy_from_slice(&128u32.to_le_bytes());

    let entries = &mut image[1024..1536];
    let ms_basic: [u8; 16] = [
        0xA2, 0xA0, 0xD0, 0xEB, 0xE5, 0xB9, 0x33, 0x44, 0x87, 0xC0, 0x68, 0xB6, 0xB7, 0x26, 0x99,
        0xC7,
    ];

    entries[0..16].copy_from_slice(&ms_basic);
    entries[32..40].copy_from_slice(&2048u64.to_le_bytes());
    entries[40..48].copy_from_slice(&3071u64.to_le_bytes());

    entries[128..144].copy_from_slice(&ms_basic);
    entries[160..168].copy_from_slice(&3072u64.to_le_bytes());
    entries[168..176].copy_from_slice(&4095u64.to_le_bytes());

    let ntfs1 = 2048usize * 512;
    image[ntfs1 + 3..ntfs1 + 11].copy_from_slice(b"NTFS    ");

    let ntfs2 = 3072usize * 512;
    image[ntfs2 + 3..ntfs2 + 11].copy_from_slice(b"NTFS    ");

    let mut cursor = Cursor::new(image);
    let probe = detect_image_filesystem(&mut cursor).unwrap();

    assert_eq!(probe.candidates.len(), 2);
    assert_eq!(probe.candidates[0].kind, ImageFilesystemKind::Ntfs);
    assert_eq!(
        probe.candidates[0].source,
        ImageFilesystemSource::GptPartition
    );
    assert_eq!(probe.candidates[0].offset, 2048 * 512);
    assert_eq!(probe.candidates[0].partition_index, Some(1));
    assert_eq!(probe.candidates[1].offset, 3072 * 512);
    assert_eq!(probe.partitions.len(), 2);
    assert!(probe
        .partitions
        .iter()
        .all(|partition| partition.status == PartitionStatus::Supported));
}

#[test]
fn detect_image_filesystem_marks_bitlocker_partition() {
    use app_services::datasource_service::{
        detect_image_filesystem, ImageFilesystemKind, PartitionStatus,
    };
    use std::io::Cursor;

    let mut image = vec![0u8; 4096 * 512];
    image[510] = 0x55;
    image[511] = 0xAA;
    image[446 + 4] = 0xEE;
    image[446 + 8..446 + 12].copy_from_slice(&1u32.to_le_bytes());
    image[446 + 12..446 + 16].copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes());

    let gpt_header = &mut image[512..1024];
    gpt_header[0..8].copy_from_slice(b"EFI PART");
    gpt_header[12..16].copy_from_slice(&92u32.to_le_bytes());
    gpt_header[72..80].copy_from_slice(&2u64.to_le_bytes());
    gpt_header[80..84].copy_from_slice(&2u32.to_le_bytes());
    gpt_header[84..88].copy_from_slice(&128u32.to_le_bytes());

    let entries = &mut image[1024..1280];
    let ms_basic: [u8; 16] = [
        0xA2, 0xA0, 0xD0, 0xEB, 0xE5, 0xB9, 0x33, 0x44, 0x87, 0xC0, 0x68, 0xB6, 0xB7, 0x26, 0x99,
        0xC7,
    ];
    entries[0..16].copy_from_slice(&ms_basic);
    entries[32..40].copy_from_slice(&2048u64.to_le_bytes());
    entries[40..48].copy_from_slice(&4095u64.to_le_bytes());

    let bitlocker = 2048usize * 512;
    image[bitlocker + 3..bitlocker + 11].copy_from_slice(b"-FVE-FS-");
    image[bitlocker + 510] = 0x55;
    image[bitlocker + 511] = 0xAA;

    let mut cursor = Cursor::new(image);
    let probe = detect_image_filesystem(&mut cursor).unwrap();

    assert!(probe.candidates.is_empty());
    assert_eq!(probe.partitions.len(), 1);
    assert_eq!(
        probe.partitions[0].filesystem,
        Some(ImageFilesystemKind::BitLocker)
    );
    assert_eq!(
        probe.partitions[0].status,
        PartitionStatus::EncryptedBitLocker
    );
    assert!(probe
        .warnings
        .iter()
        .any(|warning| warning.contains("BitLocker-encrypted")));
}
