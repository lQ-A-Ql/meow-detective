use crate::datasource_service::{
    ImageFilesystemCandidate, ImageFilesystemKind, ImageFilesystemSource, LvmLogicalVolumeIdentity,
};

fn volume(name: &str, uuid: &str) -> fs_lvm::LvInfo {
    fs_lvm::LvInfo {
        name: name.to_string(),
        uuid: uuid.to_string(),
        size_bytes: 4096,
        role: "public".to_string(),
        status: vec!["READ".to_string()],
        visible: true,
        directly_mappable: true,
        unsupported_reason: None,
    }
}

fn identity(lv_name: &str, lv_uuid: &str) -> LvmLogicalVolumeIdentity {
    LvmLogicalVolumeIdentity {
        vg_uuid: "vg-uuid".to_string(),
        vg_name: "vg".to_string(),
        lv_uuid: lv_uuid.to_string(),
        lv_name: lv_name.to_string(),
        pv_offsets: vec![0],
        pv_sources: Vec::new(),
    }
}

#[test]
fn persisted_lv_uuid_mismatch_does_not_fall_back_to_same_name() {
    let volumes = vec![volume("root", "actual-root-uuid")];

    assert_eq!(
        super::find_lvm_volume_index(&volumes, &identity("root", "missing-root-uuid")),
        None
    );
}

#[test]
fn legacy_empty_lv_uuid_uses_name_fallback() {
    let volumes = vec![volume("root", "actual-root-uuid")];

    assert_eq!(
        super::find_lvm_volume_index(&volumes, &identity("root", "")),
        Some(0)
    );
}

#[test]
fn fat_family_candidate_opens_an_exfat_reader() {
    let temp = tempfile::TempDir::new().unwrap();
    let source_path = temp.path().join("exfat.raw");
    std::fs::write(&source_path, valid_exfat_image()).unwrap();
    let candidate = ImageFilesystemCandidate {
        partition_index: Some(1),
        partition_name: Some("exfat".to_string()),
        kind: ImageFilesystemKind::Fat,
        offset: 0,
        length: None,
        source: ImageFilesystemSource::DirectVolume,
        lvm_identity: None,
    };

    let filesystem =
        super::open_candidate_filesystem(&source_path, &domain::DataSourceKind::Raw, &candidate)
            .unwrap()
            .unwrap();

    assert_eq!(filesystem.data_source_name(), "exFAT");
}

fn valid_exfat_image() -> Vec<u8> {
    const SECTOR_SIZE: usize = 512;
    const TOTAL_SECTORS: usize = 1024;
    let mut image = vec![0u8; TOTAL_SECTORS * SECTOR_SIZE];
    {
        let boot = &mut image[..SECTOR_SIZE];
        boot[..3].copy_from_slice(&[0xEB, 0x76, 0x90]);
        boot[3..11].copy_from_slice(b"EXFAT   ");
        boot[72..80].copy_from_slice(&(TOTAL_SECTORS as u64).to_le_bytes());
        boot[80..84].copy_from_slice(&24u32.to_le_bytes());
        boot[84..88].copy_from_slice(&1u32.to_le_bytes());
        boot[88..92].copy_from_slice(&32u32.to_le_bytes());
        boot[92..96].copy_from_slice(&100u32.to_le_bytes());
        boot[96..100].copy_from_slice(&2u32.to_le_bytes());
        boot[104..106].copy_from_slice(&0x0100u16.to_le_bytes());
        boot[108] = 9;
        boot[110] = 1;
        boot[510..512].copy_from_slice(&[0x55, 0xAA]);
    }
    let backup_boot = image[..SECTOR_SIZE].to_vec();
    image[12 * SECTOR_SIZE..13 * SECTOR_SIZE].copy_from_slice(&backup_boot);
    image[24 * SECTOR_SIZE + 8..24 * SECTOR_SIZE + 12]
        .copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
    write_exfat_boot_checksum(&mut image, 0, SECTOR_SIZE);
    write_exfat_boot_checksum(&mut image, 12 * SECTOR_SIZE, SECTOR_SIZE);
    image
}

fn write_exfat_boot_checksum(data: &mut [u8], region_offset: usize, sector_size: usize) {
    let mut checksum = 0u32;
    for sector_index in 0..11usize {
        let sector_offset = region_offset + sector_index * sector_size;
        for (index, byte) in data[sector_offset..sector_offset + sector_size]
            .iter()
            .enumerate()
        {
            if sector_index == 0 && matches!(index, 106 | 107 | 112) {
                continue;
            }
            checksum = checksum.rotate_right(1).wrapping_add(u32::from(*byte));
        }
    }
    for checksum_bytes in
        data[region_offset + 11 * sector_size..region_offset + 12 * sector_size].chunks_exact_mut(4)
    {
        checksum_bytes.copy_from_slice(&checksum.to_le_bytes());
    }
}
