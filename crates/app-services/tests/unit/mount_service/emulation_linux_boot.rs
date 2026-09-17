use super::*;

fn install() -> EmulationInstallDto {
    super::super::emulation_linux::linux_install_skeleton(2)
}

#[test]
fn xfs_assessment_annotations_are_fail_closed_and_deduplicated() {
    let mut installs = vec![install()];
    annotate_xfs_assessments(
        &mut installs,
        &[
            XfsLogAssessment::Clean,
            XfsLogAssessment::Dirty,
            XfsLogAssessment::Unverified,
            XfsLogAssessment::Unverified,
        ],
    );

    assert_eq!(
        installs[0].boot_risk_notes,
        vec!["xfs-log-dirty", "xfs-log-unverified"]
    );
}

#[test]
fn clean_or_absent_xfs_assessments_add_no_risk() {
    for assessments in [Vec::new(), vec![XfsLogAssessment::Clean]] {
        let mut installs = vec![install()];
        annotate_xfs_assessments(&mut installs, &assessments);
        assert!(installs[0].boot_risk_notes.is_empty());
    }
}

#[test]
fn ext4_journal_assessment_annotations_are_fail_closed_and_deduplicated() {
    let mut installs = vec![install()];
    annotate_ext4_assessments(
        &mut installs,
        &[
            Ext4JournalAssessment::Clean,
            Ext4JournalAssessment::Dirty,
            Ext4JournalAssessment::Unverified,
            Ext4JournalAssessment::Unverified,
        ],
    );
    assert_eq!(
        installs[0].boot_risk_notes,
        vec!["ext4-journal-dirty", "ext4-journal-unverified"]
    );
}

#[test]
fn clean_or_absent_ext4_journal_assessments_add_no_risk() {
    for assessments in [Vec::new(), vec![Ext4JournalAssessment::Clean]] {
        let mut installs = vec![install()];
        annotate_ext4_assessments(&mut installs, &assessments);
        assert!(installs[0].boot_risk_notes.is_empty());
    }
}

#[test]
fn boot_path_annotations_are_fail_closed_and_deduplicated() {
    let mut installs = vec![install()];
    annotate_boot_path_assessment(&mut installs, BootPathAssessment::EspUnverified);
    annotate_boot_path_assessment(&mut installs, BootPathAssessment::EspUnverified);
    annotate_boot_path_assessment(&mut installs, BootPathAssessment::NoEfiFallback);
    assert_eq!(
        installs[0].boot_risk_notes,
        vec!["esp-unverified", "no-efi-fallback"]
    );

    let mut installs = vec![install()];
    for assessment in [
        BootPathAssessment::BootPathPresent,
        BootPathAssessment::Undetermined,
    ] {
        annotate_boot_path_assessment(&mut installs, assessment);
    }
    assert!(installs[0].boot_risk_notes.is_empty());
}

const SECTOR: usize = 512;
const ESP_START_LBA: u64 = 2048;
const ESP_SECTORS: u64 = 264;
const DISK_SECTORS: u64 = ESP_START_LBA + ESP_SECTORS + 33;
const ESP_TYPE_GUID: [u8; 16] = [
    0x28, 0x73, 0x2A, 0xC1, 0x1F, 0xF8, 0xD2, 0x11, 0xBA, 0x4B, 0x00, 0xA0, 0xC9, 0x3E, 0xC9, 0x3B,
];

fn fat_pos(fat_index: usize, cluster: u32) -> usize {
    (32 + fat_index * 16) * SECTOR + cluster as usize * 4
}

fn cluster_pos(cluster: u32) -> usize {
    (64 + (cluster as usize - 2)) * SECTOR
}

fn put_dir_entry(
    data: &mut [u8],
    slot: usize,
    name: &str,
    ext: &str,
    attr: u8,
    cluster: u32,
    size: u32,
) {
    let entry = &mut data[slot * 32..(slot + 1) * 32];
    entry.fill(0);
    entry[..8].fill(b' ');
    entry[..name.len()].copy_from_slice(name.as_bytes());
    entry[8..11].fill(b' ');
    entry[8..8 + ext.len()].copy_from_slice(ext.as_bytes());
    entry[11] = attr;
    entry[20..22].copy_from_slice(&((cluster >> 16) as u16).to_le_bytes());
    entry[26..28].copy_from_slice(&(cluster as u16).to_le_bytes());
    entry[28..32].copy_from_slice(&size.to_le_bytes());
}

/// A minimal FAT32 ESP; `with_fallback` adds `\EFI\BOOT\BOOTX64.EFI`.
fn esp_image(with_fallback: bool) -> Vec<u8> {
    let mut data = vec![0u8; ESP_SECTORS as usize * SECTOR];
    data[11..13].copy_from_slice(&(SECTOR as u16).to_le_bytes());
    data[13] = 1;
    data[14..16].copy_from_slice(&32u16.to_le_bytes());
    data[16] = 2;
    data[32..36].copy_from_slice(&(ESP_SECTORS as u32).to_le_bytes());
    data[36..40].copy_from_slice(&16u32.to_le_bytes());
    data[44..48].copy_from_slice(&2u32.to_le_bytes());
    data[66] = 0x29;
    data[SECTOR..SECTOR + 4].copy_from_slice(&0x4161_5252u32.to_le_bytes());
    data[SECTOR + 484..SECTOR + 488].copy_from_slice(&0x6141_7272u32.to_le_bytes());
    data[SECTOR + 488..SECTOR + 492].copy_from_slice(&190u32.to_le_bytes());
    for fat in 0..2 {
        for cluster in 0..=5u32 {
            let value: u32 = if cluster == 0 {
                0x0FFF_FFF8
            } else {
                0x0FFF_FFFF
            };
            data[fat_pos(fat, cluster)..fat_pos(fat, cluster) + 4]
                .copy_from_slice(&value.to_le_bytes());
        }
    }
    let root = cluster_pos(2);
    put_dir_entry(&mut data[root..root + SECTOR], 0, "EFI", "", 0x10, 3, 0);
    let efi = cluster_pos(3);
    put_dir_entry(&mut data[efi..efi + SECTOR], 0, ".", "", 0x10, 3, 0);
    put_dir_entry(&mut data[efi..efi + SECTOR], 1, "..", "", 0x10, 0, 0);
    if with_fallback {
        put_dir_entry(&mut data[efi..efi + SECTOR], 2, "BOOT", "", 0x10, 4, 0);
        let boot = cluster_pos(4);
        put_dir_entry(&mut data[boot..boot + SECTOR], 0, ".", "", 0x10, 4, 0);
        put_dir_entry(&mut data[boot..boot + SECTOR], 1, "..", "", 0x10, 3, 0);
        put_dir_entry(
            &mut data[boot..boot + SECTOR],
            2,
            "BOOTX64",
            "EFI",
            0x20,
            5,
            9,
        );
    }
    data
}

/// A GPT disk with the given partitions and no BIOS boot partition unless
/// `bios_boot` is set.
fn gpt_disk_image(esp: Option<&[u8]>, bios_boot: bool) -> Vec<u8> {
    let mut disk = vec![0u8; DISK_SECTORS as usize * SECTOR];
    disk[510] = 0x55;
    disk[511] = 0xAA;
    disk[446 + 4] = 0xEE;
    disk[446 + 8..446 + 12].copy_from_slice(&1u32.to_le_bytes());
    disk[446 + 12..446 + 16].copy_from_slice(&(DISK_SECTORS as u32 - 1).to_le_bytes());
    let header = SECTOR;
    disk[header..header + 8].copy_from_slice(b"EFI PART");
    disk[header + 12..header + 16].copy_from_slice(&92u32.to_le_bytes());
    disk[header + 40..header + 48].copy_from_slice(&34u64.to_le_bytes());
    disk[header + 48..header + 56].copy_from_slice(&(DISK_SECTORS - 34).to_le_bytes());
    disk[header + 72..header + 80].copy_from_slice(&2u64.to_le_bytes());
    disk[header + 80..header + 84].copy_from_slice(&128u32.to_le_bytes());
    disk[header + 84..header + 88].copy_from_slice(&128u32.to_le_bytes());
    let mut slot = 0usize;
    if let Some(esp) = esp {
        let entry = 2 * SECTOR;
        disk[entry..entry + 16].copy_from_slice(&ESP_TYPE_GUID);
        disk[entry + 16..entry + 32].copy_from_slice(&[7u8; 16]);
        disk[entry + 32..entry + 40].copy_from_slice(&ESP_START_LBA.to_le_bytes());
        disk[entry + 40..entry + 48]
            .copy_from_slice(&(ESP_START_LBA + ESP_SECTORS - 1).to_le_bytes());
        let offset = ESP_START_LBA as usize * SECTOR;
        disk[offset..offset + esp.len()].copy_from_slice(esp);
        slot = 1;
    }
    if bios_boot {
        let entry = 2 * SECTOR + slot * 128;
        disk[entry..entry + 16].copy_from_slice(b"Hah!IdontNeedEFI");
        disk[entry + 32..entry + 40].copy_from_slice(&4096u64.to_le_bytes());
        disk[entry + 40..entry + 48].copy_from_slice(&8191u64.to_le_bytes());
    }
    disk
}

fn assess_disk(image: &[u8]) -> BootPathAssessment {
    let temp = tempfile::TempDir::new().unwrap();
    let path = temp.path().join("source.raw");
    std::fs::write(&path, image).unwrap();
    gpt_disk_boot_path_assessment(&path, &domain::DataSourceKind::Raw)
}

#[test]
fn gpt_disk_with_bios_boot_partition_has_a_boot_path() {
    let image = gpt_disk_image(None, true);
    assert_eq!(assess_disk(&image), BootPathAssessment::BootPathPresent);
}

#[test]
fn gpt_disk_without_esp_is_missing_the_efi_fallback() {
    let image = gpt_disk_image(None, false);
    assert_eq!(assess_disk(&image), BootPathAssessment::NoEfiFallback);
}

#[test]
fn esp_without_fallback_loader_is_missing_the_efi_fallback() {
    let image = gpt_disk_image(Some(&esp_image(false)), false);
    assert_eq!(assess_disk(&image), BootPathAssessment::NoEfiFallback);
}

#[test]
fn esp_with_fallback_loader_has_a_boot_path() {
    let image = gpt_disk_image(Some(&esp_image(true)), false);
    assert_eq!(assess_disk(&image), BootPathAssessment::BootPathPresent);
}

#[test]
fn unopenable_esp_is_reported_unverified() {
    let mut esp = esp_image(true);
    esp[..SECTOR].fill(0);
    let image = gpt_disk_image(Some(&esp), false);
    assert_eq!(assess_disk(&image), BootPathAssessment::EspUnverified);
}

#[test]
fn mbr_disk_layout_is_undetermined() {
    let mut image = vec![0u8; 2 * SECTOR];
    image[510] = 0x55;
    image[511] = 0xAA;
    assert_eq!(assess_disk(&image), BootPathAssessment::Undetermined);
}
