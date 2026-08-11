use super::*;

use evidence_emulation::{CowDiskConfig, ParentIdentity};

const BPS: usize = 512;
const ESP_START_LBA: u64 = 2048;
const ESP_SECTORS: u64 = 264;
const DISK_SECTORS: u64 = ESP_START_LBA + ESP_SECTORS + 33;

fn fat_cluster_pos(cluster: u32) -> usize {
    // Mirrors the fixture layout: reserved 32, two 16-sector FATs.
    (64 + (cluster as usize - 2)) * BPS
}

fn fat_pos(fat_index: usize, cluster: u32) -> usize {
    (32 + fat_index * 16) * BPS + cluster as usize * 4
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
    for byte in entry.iter_mut() {
        *byte = 0;
    }
    entry[..8].fill(b' ');
    entry[..name.len()].copy_from_slice(name.as_bytes());
    entry[8..11].fill(b' ');
    entry[8..8 + ext.len()].copy_from_slice(ext.as_bytes());
    entry[11] = attr;
    entry[20..22].copy_from_slice(&((cluster >> 16) as u16).to_le_bytes());
    entry[26..28].copy_from_slice(&(cluster as u16).to_le_bytes());
    entry[28..32].copy_from_slice(&size.to_le_bytes());
}

/// A FAT32 ESP with `\EFI\KALI\GRUBX64.EFI` and no `\EFI\BOOT` fallback.
fn esp_image() -> Vec<u8> {
    let mut data = vec![0u8; ESP_SECTORS as usize * BPS];
    data[11..13].copy_from_slice(&(BPS as u16).to_le_bytes());
    data[13] = 1;
    data[14..16].copy_from_slice(&32u16.to_le_bytes());
    data[16] = 2;
    data[32..36].copy_from_slice(&(ESP_SECTORS as u32).to_le_bytes());
    data[36..40].copy_from_slice(&16u32.to_le_bytes());
    data[44..48].copy_from_slice(&2u32.to_le_bytes());
    data[66] = 0x29;
    data[BPS..BPS + 4].copy_from_slice(&0x4161_5252u32.to_le_bytes());
    data[BPS + 484..BPS + 488].copy_from_slice(&0x6141_7272u32.to_le_bytes());
    data[BPS + 488..BPS + 492].copy_from_slice(&190u32.to_le_bytes());
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
    let root = fat_cluster_pos(2);
    put_dir_entry(&mut data[root..root + BPS], 0, "EFI", "", 0x10, 3, 0);
    let efi = fat_cluster_pos(3);
    put_dir_entry(&mut data[efi..efi + BPS], 0, ".", "", 0x10, 3, 0);
    put_dir_entry(&mut data[efi..efi + BPS], 1, "..", "", 0x10, 0, 0);
    put_dir_entry(&mut data[efi..efi + BPS], 2, "KALI", "", 0x10, 4, 0);
    let kali = fat_cluster_pos(4);
    put_dir_entry(&mut data[kali..kali + BPS], 0, ".", "", 0x10, 4, 0);
    put_dir_entry(&mut data[kali..kali + BPS], 1, "..", "", 0x10, 3, 0);
    put_dir_entry(&mut data[kali..kali + BPS], 2, "GRUBX64", "EFI", 0x20, 5, 9);
    let grub = fat_cluster_pos(5);
    data[grub..grub + 9].copy_from_slice(b"grub-core");
    data
}

/// A GPT disk whose only partition is the ESP above (no BIOS boot
/// partition, no fallback loader — the Kali-style unbootable layout).
fn gpt_disk_image() -> Vec<u8> {
    let mut disk = vec![0u8; DISK_SECTORS as usize * BPS];
    disk[510] = 0x55;
    disk[511] = 0xAA;
    disk[446 + 4] = 0xEE;
    disk[446 + 8..446 + 12].copy_from_slice(&1u32.to_le_bytes());
    disk[446 + 12..446 + 16].copy_from_slice(&(DISK_SECTORS as u32 - 1).to_le_bytes());
    let header = BPS;
    disk[header..header + 8].copy_from_slice(b"EFI PART");
    disk[header + 12..header + 16].copy_from_slice(&92u32.to_le_bytes());
    disk[header + 40..header + 48].copy_from_slice(&34u64.to_le_bytes());
    disk[header + 48..header + 56].copy_from_slice(&(DISK_SECTORS - 34).to_le_bytes());
    disk[header + 72..header + 80].copy_from_slice(&2u64.to_le_bytes());
    disk[header + 80..header + 84].copy_from_slice(&128u32.to_le_bytes());
    disk[header + 84..header + 88].copy_from_slice(&128u32.to_le_bytes());
    let entry = 2 * BPS;
    let esp_type: [u8; 16] = [
        0x28, 0x73, 0x2A, 0xC1, 0x1F, 0xF8, 0xD2, 0x11, 0xBA, 0x4B, 0x00, 0xA0, 0xC9, 0x3E, 0xC9,
        0x3B,
    ];
    disk[entry..entry + 16].copy_from_slice(&esp_type);
    disk[entry + 16..entry + 32].copy_from_slice(&[7u8; 16]);
    disk[entry + 32..entry + 40].copy_from_slice(&ESP_START_LBA.to_le_bytes());
    disk[entry + 40..entry + 48].copy_from_slice(&(ESP_START_LBA + ESP_SECTORS - 1).to_le_bytes());
    let esp_offset = ESP_START_LBA as usize * BPS;
    disk[esp_offset..esp_offset + esp_image().len()].copy_from_slice(&esp_image());
    disk
}

fn session_disk(image: &[u8], temp: &tempfile::TempDir) -> Arc<CowDisk> {
    let image_path = temp.path().join("source.raw");
    std::fs::write(&image_path, image).unwrap();
    let provider =
        evidence_block::open_block_provider(&image_path, evidence_block::EvidenceImageKind::Raw)
            .unwrap();
    let identity = ParentIdentity::new(provider.len(), [9u8; 32]).unwrap();
    Arc::new(
        CowDisk::create(
            &temp.path().join("overlay.cow"),
            provider,
            identity,
            CowDiskConfig::default(),
        )
        .unwrap(),
    )
}

fn esp_reader(disk: &Arc<CowDisk>) -> fs_fat::FatReader {
    let window = PartitionWindowReader::new(
        Box::new(CowDiskReader::new(Arc::clone(disk))) as Box<dyn EvidenceReader>,
        ESP_START_LBA * BPS as u64,
        Some(ESP_SECTORS * BPS as u64),
    )
    .unwrap();
    fs_fat::FatReader::open(Box::new(window), 0).unwrap()
}

#[test]
fn installs_grub_fallback_through_the_overlay() {
    let temp = tempfile::TempDir::new().unwrap();
    let image = gpt_disk_image();
    let disk = session_disk(&image, &temp);

    let result = install_efi_fallback(&disk, "ds-test").unwrap();
    assert!(!result.already_present);
    assert_eq!(result.strategy, Some(EmulationEfiFallbackStrategyDto::Grub));
    assert_eq!(result.files_written, ["BOOTX64.EFI"]);
    assert_eq!(result.esp_partition_index, 1);

    // The fallback loader resolves through a fresh view of the overlay.
    let fs = esp_reader(&disk);
    assert_eq!(
        fs.read_file_range("EFI/BOOT/BOOTX64.EFI", 0, 64).unwrap(),
        b"grub-core"
    );

    // The evidence image is byte-identical.
    assert_eq!(
        std::fs::read(temp.path().join("source.raw")).unwrap(),
        image
    );

    // A second run is a no-op.
    let second = install_efi_fallback(&disk, "ds-test").unwrap();
    assert!(second.already_present);
    assert!(second.files_written.is_empty());
}

#[test]
fn rejects_mbr_disks() {
    let temp = tempfile::TempDir::new().unwrap();
    let mut image = vec![0u8; 2 * BPS];
    image[510] = 0x55;
    image[511] = 0xAA;
    let disk = session_disk(&image, &temp);
    let error = install_efi_fallback(&disk, "ds-test").unwrap_err();
    assert!(matches!(error, EmulationBypassError::Unsupported(_)));
}
