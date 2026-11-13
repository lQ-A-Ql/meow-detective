#[path = "support/initialized_data.rs"]
mod support;

use evidence_core::FileSystemReader;
use std::io::{Read, Seek, SeekFrom};
use support::{filesystem, image, ATTRIBUTE_OFFSET};

fn empty_resident_image() -> Vec<u8> {
    let mut fixture = image(0, 0, false);
    let attribute = &mut fixture[ATTRIBUTE_OFFSET..ATTRIBUTE_OFFSET + 72];
    attribute.fill(0);
    attribute[..4].copy_from_slice(&0x80u32.to_le_bytes());
    attribute[4..8].copy_from_slice(&24u32.to_le_bytes());
    attribute[20..22].copy_from_slice(&24u16.to_le_bytes());
    attribute[24..28].copy_from_slice(&u32::MAX.to_le_bytes());
    fixture
}

fn no_allocation_image(real_size: u64) -> Vec<u8> {
    let mut fixture = image(real_size, real_size, false);
    let attribute = &mut fixture[ATTRIBUTE_OFFSET..ATTRIBUTE_OFFSET + 72];
    attribute[24..32].copy_from_slice(&0u64.to_le_bytes());
    attribute[40..48].copy_from_slice(&0u64.to_le_bytes());
    attribute[64..72].fill(0);
    fixture
}

fn assert_empty_file(fixture: &[u8]) {
    let (reader, directory) = filesystem(fixture);
    assert_eq!(reader.file_size_by_inode(6).unwrap(), Some(0));
    assert!(reader.supports_file_stream_by_inode(6).unwrap());
    let mut bytes = Vec::new();
    reader
        .open_file("mft:6")
        .unwrap()
        .read_to_end(&mut bytes)
        .unwrap();
    assert_eq!(bytes, support::expected(0, 0));
    for offset in [0, 1, u64::MAX] {
        assert!(reader
            .read_file_range_by_inode(6, offset, 4096)
            .unwrap()
            .is_empty());
    }
    let mut stream = reader.into_file_stream_by_inode(6).unwrap();
    assert_eq!(stream.seek(SeekFrom::End(0)).unwrap(), 0);
    assert_eq!(stream.read(&mut [0xff; 16]).unwrap(), 0);
    assert_eq!(stream.seek(SeekFrom::Start(64)).unwrap(), 64);
    assert_eq!(stream.read(&mut [0xff; 16]).unwrap(), 0);
    assert_eq!(
        std::fs::read(directory.path().join("initialized-data.raw")).unwrap(),
        fixture
    );
}

fn assert_invalid_file(fixture: &[u8]) {
    let (reader, _directory) = filesystem(fixture);
    assert_eq!(
        reader.open_file("mft:6").err().unwrap().kind(),
        std::io::ErrorKind::InvalidData
    );
    assert_eq!(
        reader
            .read_file_range_by_inode(6, 0, 16)
            .unwrap_err()
            .kind(),
        std::io::ErrorKind::InvalidData
    );
    assert_eq!(
        reader.supports_file_stream_by_inode(6).unwrap_err().kind(),
        std::io::ErrorKind::InvalidData
    );
    assert_eq!(
        reader.into_file_stream_by_inode(6).err().unwrap().kind(),
        std::io::ErrorKind::InvalidData
    );
}

#[test]
fn resident_zero_length_file_returns_eof_on_all_read_paths() {
    assert_empty_file(&empty_resident_image());
}

#[test]
fn nonresident_zero_length_file_without_data_runs_returns_eof() {
    for highest_vcn in [0, u64::MAX] {
        let mut fixture = no_allocation_image(0);
        fixture[ATTRIBUTE_OFFSET + 24..ATTRIBUTE_OFFSET + 32]
            .copy_from_slice(&highest_vcn.to_le_bytes());
        assert_empty_file(&fixture);
    }
}

#[test]
fn nonzero_file_without_data_runs_remains_invalid_data() {
    assert_invalid_file(&no_allocation_image(16));
}

#[test]
fn resident_nonzero_value_outside_its_attribute_remains_invalid_data() {
    let mut fixture = empty_resident_image();
    fixture[ATTRIBUTE_OFFSET + 16..ATTRIBUTE_OFFSET + 20].copy_from_slice(&1u32.to_le_bytes());
    assert_invalid_file(&fixture);
}

#[test]
fn empty_stream_with_an_unmapped_allocation_remains_invalid_data() {
    let mut fixture = no_allocation_image(0);
    fixture[ATTRIBUTE_OFFSET + 40..ATTRIBUTE_OFFSET + 48].copy_from_slice(&512u64.to_le_bytes());
    assert_invalid_file(&fixture);
}
