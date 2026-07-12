use super::*;
use std::io::Cursor;

/// Build a minimal disk image with sector 0 (empty) + sector 1 (PV label).
fn build_label_disk(pv_uuid: &str, pv_size: u64) -> Vec<u8> {
    build_label_disk_at_sector(pv_uuid, pv_size, 1)
}

fn build_label_disk_at_sector(pv_uuid: &str, pv_size: u64, sector_index: u64) -> Vec<u8> {
    let mut disk = vec![0u8; 1024]; // sector 0 empty, sector 1 = label
    let label_offset = sector_index as usize * LABEL_SECTOR_SIZE;
    disk.resize(label_offset + LABEL_SECTOR_SIZE, 0);
    let sector = &mut disk[label_offset..label_offset + LABEL_SECTOR_SIZE];
    // label header
    sector[0..8].copy_from_slice(b"LABELONE");
    sector[8..16].copy_from_slice(&sector_index.to_le_bytes()); // sector_number
                                                                // crc at 16..20, filled after
    sector[20..24].copy_from_slice(&32u32.to_le_bytes()); // data_offset
    sector[24..32].copy_from_slice(b"LVM2 001");

    // pv header at offset 32
    let uuid_bytes = format!("{:32}", pv_uuid); // pad to 32
    sector[32..64].copy_from_slice(&uuid_bytes.as_bytes()[..32]);
    sector[64..72].copy_from_slice(&pv_size.to_le_bytes());

    // one data area
    sector[72..80].copy_from_slice(&2048u64.to_le_bytes()); // offset
    sector[80..88].copy_from_slice(&(pv_size - 2048).to_le_bytes()); // size
                                                                     // terminator
                                                                     // (bytes 88..104 already zero)

    // one metadata area
    sector[104..112].copy_from_slice(&512u64.to_le_bytes()); // offset=512 (sector 1)
    sector[112..120].copy_from_slice(&(255 * 512u64).to_le_bytes()); // size
                                                                     // terminator
                                                                     // (bytes 120..136 already zero)

    // Compute and write CRC-32 of bytes 20..512
    let crc = crc::lvm_crc32(&sector[20..512]);
    sector[16..20].copy_from_slice(&crc.to_le_bytes());

    disk
}

fn fake_reader(data: Vec<u8>) -> impl Read + Seek {
    // Ensure at least 1024 bytes so sector 1 is readable
    let mut padded = data;
    if padded.len() < 1024 {
        padded.resize(1024, 0);
    }
    Cursor::new(padded)
}

fn refresh_label_crc(disk: &mut [u8], sector_index: u64) {
    let label_offset = sector_index as usize * LABEL_SECTOR_SIZE;
    let sector = &mut disk[label_offset..label_offset + LABEL_SECTOR_SIZE];
    let crc = crc::lvm_crc32(&sector[20..512]);
    sector[16..20].copy_from_slice(&crc.to_le_bytes());
}

#[test]
fn parse_valid_label() {
    let disk = build_label_disk("9LBcEB7PQTGIlLI0KxrtzrynjuSL983W", 10_737_418_240);
    let mut reader = fake_reader(disk);
    let label = parse_pv_label(&mut reader, 0).unwrap();

    assert_eq!(label.pv_uuid, "9LBcEB7PQTGIlLI0KxrtzrynjuSL983W");
    assert_eq!(label.pv_size, 10_737_418_240);
    assert_eq!(label.data_areas.len(), 1);
    assert_eq!(label.data_areas[0].offset, 2048);
    assert_eq!(label.metadata_areas.len(), 1);
    assert_eq!(label.metadata_areas[0].offset, 512);
}

#[test]
fn parse_label_in_first_four_scan_sectors() {
    let disk = build_label_disk_at_sector("9LBcEB7PQTGIlLI0KxrtzrynjuSL983W", 10_737_418_240, 3);
    let mut reader = fake_reader(disk);
    let label = parse_pv_label(&mut reader, 0).unwrap();

    assert_eq!(label.pv_uuid, "9LBcEB7PQTGIlLI0KxrtzrynjuSL983W");
    assert_eq!(label.metadata_areas.len(), 1);
}

#[test]
fn ignores_label_beyond_first_four_scan_sectors() {
    let disk = build_label_disk_at_sector("9LBcEB7PQTGIlLI0KxrtzrynjuSL983W", 10_737_418_240, 4);
    let mut reader = fake_reader(disk);
    let err = parse_pv_label(&mut reader, 0).unwrap_err();

    assert!(matches!(err, LvmError::NotLvm));
}

#[test]
fn parse_label_at_partition_offset() {
    // PV starts at LBA 2048 (1 MB into the disk)
    let label_data = build_label_disk("abcd1234abcd1234abcd1234abcd1234", 5_000_000_000);
    // Need at least 2 sectors: padding + label sector (offset 0) + label sector (offset 512)
    let mut disk = vec![0u8; 2048 * 512]; // padding before PV
    disk.extend(label_data); // PV sector 0 + sector 1 (1024 bytes)

    let pv_offset = 2048 * 512;
    let mut reader = fake_reader(disk);
    let label = parse_pv_label(&mut reader, pv_offset).unwrap();

    assert_eq!(label.pv_uuid, "abcd1234abcd1234abcd1234abcd1234");
    assert_eq!(label.pv_size, 5_000_000_000);
}

#[test]
fn reject_non_lvm_sector() {
    let mut disk = vec![0u8; 2048]; // at least 1024
    disk[512..520].copy_from_slice(b"NOTALABE"); // exactly 8 bytes
    let mut reader = fake_reader(disk);
    let err = parse_pv_label(&mut reader, 0).unwrap_err();
    assert!(matches!(err, LvmError::NotLvm));
}

#[test]
fn reject_bad_crc() {
    let mut disk = build_label_disk("test1234test1234test1234test1234", 1_000_000);
    disk.resize(1024, 0);
    // Corrupt byte 600 (sector byte 88, well within CRC region, past magic bytes)
    disk[600] ^= 0xFF;
    let mut reader = fake_reader(disk);
    let err = parse_pv_label(&mut reader, 0).unwrap_err();
    assert!(matches!(err, LvmError::LabelCrcMismatch { .. }));
}

#[test]
fn reject_missing_metadata_descriptor_terminator() {
    let mut disk = build_label_disk("test1234test1234test1234test1234", 1_000_000);
    let label_offset = LABEL_SECTOR_SIZE;
    let sector = &mut disk[label_offset..label_offset + LABEL_SECTOR_SIZE];
    for offset in (120..LABEL_SECTOR_SIZE).step_by(16) {
        sector[offset..offset + 8].copy_from_slice(&1u64.to_le_bytes());
    }
    refresh_label_crc(&mut disk, 1);

    let mut reader = fake_reader(disk);
    let err = parse_pv_label(&mut reader, 0).unwrap_err();

    assert!(matches!(err, LvmError::MetadataParseError { .. }));
}

#[test]
fn reject_metadata_descriptor_start_outside_label_sector() {
    let mut disk = build_label_disk("test1234test1234test1234test1234", 1_000_000);
    let label_offset = LABEL_SECTOR_SIZE;
    let sector = &mut disk[label_offset..label_offset + LABEL_SECTOR_SIZE];
    for offset in (88..LABEL_SECTOR_SIZE).step_by(16) {
        sector[offset..offset + 8].copy_from_slice(&1u64.to_le_bytes());
    }
    refresh_label_crc(&mut disk, 1);

    let mut reader = fake_reader(disk);
    let err = parse_pv_label(&mut reader, 0).unwrap_err();

    assert!(matches!(err, LvmError::MetadataParseError { .. }));
}
