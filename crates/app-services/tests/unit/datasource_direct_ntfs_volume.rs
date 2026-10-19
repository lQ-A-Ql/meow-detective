use super::probe;
use std::io::{Cursor, Seek, SeekFrom};

fn image(bps: u16, backup_lba: u64) -> Vec<u8> {
    let mut bytes = vec![0; 128 * bps as usize];
    let boot = &mut bytes[..512];
    boot[3..11].copy_from_slice(b"NTFS    ");
    boot[11..13].copy_from_slice(&bps.to_le_bytes());
    boot[13] = 8;
    boot[28..32].copy_from_slice(&649216u32.to_le_bytes());
    boot[40..48].copy_from_slice(&64u64.to_le_bytes());
    boot[48..56].copy_from_slice(&2u64.to_le_bytes());
    boot[72..80].copy_from_slice(&0x4482D035A65A1162u64.to_le_bytes());
    boot[510..512].copy_from_slice(&[0x55, 0xaa]);
    let backup = bytes[..512].to_vec();
    let start = backup_lba as usize * bps as usize;
    bytes[start..start + 512].copy_from_slice(&backup);
    bytes
}

#[test]
fn includes_verified_backup_after_declared_ntfs_sector_range() {
    let mut reader = Cursor::new(image(512, 64));
    reader.seek(SeekFrom::Start(17)).unwrap();
    let geometry = probe(&mut reader).unwrap().unwrap();
    assert_eq!(geometry.declared_volume_bytes, 64 * 512);
    assert_eq!(geometry.verified_extent_bytes, Some(65 * 512));
    assert_eq!(geometry.logical_media_bytes, 128 * 512);
    assert_eq!(geometry.volume_serial, "4482D035A65A1162");
    assert_eq!(reader.position(), 17);
    let result = crate::datasource_service::detect_image_filesystem(&mut reader).unwrap();
    assert_eq!(result.partitions[0].offset, 0);
    assert_eq!(result.partitions[0].length, 65 * 512);
    assert_eq!(result.candidates[0].length, Some(65 * 512));
    assert_eq!(result.partitions[0].uuid, None);
}

#[test]
fn four_k_sector_volume_keeps_matching_backup_inside_declared_extent() {
    let mut reader = Cursor::new(image(4096, 63));
    let geometry = probe(&mut reader).unwrap().unwrap();
    assert_eq!(geometry.declared_volume_bytes, 64 * 4096);
    assert_eq!(geometry.verified_extent_bytes, Some(64 * 4096));
}

#[test]
fn missing_backup_keeps_declared_and_verified_geometry_distinct() {
    let mut bytes = image(512, 64);
    bytes[64 * 512..65 * 512].fill(0);
    let geometry = probe(&mut Cursor::new(bytes)).unwrap().unwrap();
    assert_eq!(geometry.declared_volume_bytes, 32768);
    assert_eq!(geometry.verified_extent_bytes, None);
}

#[test]
fn oversized_volume_or_mft_fails_without_trusting_padded_media() {
    let mut bytes = image(512, 64);
    bytes[40..48].copy_from_slice(&u64::MAX.to_le_bytes());
    assert!(probe(&mut Cursor::new(bytes)).is_err());
    let mut bytes = image(512, 64);
    bytes[48..56].copy_from_slice(&u64::MAX.to_le_bytes());
    assert!(probe(&mut Cursor::new(bytes)).is_err());
}

#[test]
fn disk_or_short_input_is_not_misclassified_as_a_direct_ntfs_volume() {
    assert!(probe(&mut Cursor::new(vec![0; 1024])).unwrap().is_none());
    assert!(probe(&mut Cursor::new(vec![0; 100])).unwrap().is_none());
}
