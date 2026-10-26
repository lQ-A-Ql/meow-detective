#[path = "support/mft_metadata.rs"]
mod mft_metadata;

use fs_ntfs::{
    apply_record_fixup, mft_scanner::MftRecordParser, parse_data_runs_bytes, parse_data_runs_ext,
    parse_mft_data_attribute, ValidatedRecord,
};

fn image() -> Vec<u8> {
    let mut bytes = vec![0u8; 16 * 1024];
    bytes[3..11].copy_from_slice(b"NTFS    ");
    bytes[11..13].copy_from_slice(&512u16.to_le_bytes());
    bytes[13] = 1;
    bytes[48..56].copy_from_slice(&2u64.to_le_bytes());
    bytes[64] = (-10i8) as u8;
    mft_metadata::initialize(&mut bytes);
    bytes
}

#[test]
fn shared_mft_metadata_and_scanner_use_the_same_record_validation() {
    let image = image();
    let mut raw = image[1024..2048].to_vec();
    let valid = ValidatedRecord::new(&mut raw, 512).unwrap();
    let data = valid.mft_data().unwrap();
    assert_eq!(data.real_size, 10 * 1024);
    assert_eq!(data.runs, vec![(2, 20)]);
    assert_eq!(
        parse_mft_data_attribute(valid.bytes()).unwrap().runs,
        data.runs
    );
    let mut parser = MftRecordParser::new(1024, 512);
    assert!(parser.parse(&image[1024..2048], 0).is_some());
    let mut corrupted = image[1024..2048].to_vec();
    corrupted[1022] ^= 1;
    let before = corrupted.clone();
    assert!(apply_record_fixup(&mut corrupted, 512).is_err());
    assert_eq!(
        corrupted, before,
        "validation failure must leave bytes unchanged"
    );
    assert!(parser.parse(&corrupted, 0).is_none());
}

#[test]
fn fixup_uses_format_blocks_on_4kn_and_rejects_out_of_range_array() {
    let bytes = image();
    let mut record = bytes[1024..2048].to_vec();
    assert!(ValidatedRecord::new(&mut record, 4096).is_ok());
    let mut record = bytes[1024..2048].to_vec();
    record[6..8].copy_from_slice(&4u16.to_le_bytes());
    let before = record.clone();
    assert!(apply_record_fixup(&mut record, 512).is_err());
    assert_eq!(record, before);
}

#[test]
fn data_runs_preserve_sparse_ranges_and_allocated_metadata_rejects_them() {
    let runs = [0x11, 2, 5, 0x01, 3, 0x11, 1, 2, 0];
    let decoded = parse_data_runs_ext(&runs).unwrap();
    assert_eq!(decoded[0].lcn, Some(5));
    assert_eq!(decoded[1].lcn, None);
    assert_eq!(decoded[1].cluster_count, 3);
    assert_eq!(decoded[2].lcn, Some(7));
    assert!(parse_data_runs_bytes(&runs).is_err());
    for malformed in [
        &[0x11, 2][..],
        &[0x10, 2, 1],
        &[0x11, 0, 1, 0],
        &[0x11, 1, 255, 0],
    ] {
        assert!(parse_data_runs_ext(malformed).is_err());
    }
}

#[test]
fn record_size_uses_volume_geometry_and_checks_invalid_encodings() {
    let mut bytes = image();
    bytes[11..13].copy_from_slice(&4096u16.to_le_bytes());
    bytes[64] = 1;
    assert_eq!(fs_ntfs::mft_record_bytes(&bytes).unwrap(), 4096);
    bytes[64] = (-10i8) as u8;
    assert_eq!(fs_ntfs::mft_record_bytes(&bytes).unwrap(), 1024);
    bytes[64] = 0;
    assert!(fs_ntfs::mft_record_bytes(&bytes).is_err());
    bytes[64] = (-31i8) as u8;
    assert_eq!(
        fs_ntfs::mft_record_bytes(&bytes).unwrap_err().kind(),
        std::io::ErrorKind::OutOfMemory
    );
    assert!(fs_ntfs::mft_record_bytes(&[]).is_err());
}
