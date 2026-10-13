//! Logical NTFS reads preserve EOF and hide physical bytes beyond valid data length.

#[path = "support/initialized_data.rs"]
mod support;

use evidence_core::FileSystemReader;
use std::io::{Read, Seek, SeekFrom};
use support::{expected, filesystem, image, ATTRIBUTE_OFFSET, DATA_OFFSET};

#[test]
fn buffered_and_range_reads_zero_uninitialized_tail_without_changing_eof() {
    let (reader, _directory) = filesystem(&image(900, 600, false));
    let mut bytes = Vec::new();
    reader
        .open_file("mft:6")
        .unwrap()
        .read_to_end(&mut bytes)
        .unwrap();
    let oracle = expected(900, 600);
    assert_eq!(bytes, oracle);
    assert_eq!(reader.file_size_by_inode(6).unwrap(), Some(900));
    assert_eq!(
        reader.read_file_range_by_inode(6, 590, 40).unwrap(),
        oracle[590..630]
    );
    assert_eq!(
        reader.read_file_range_by_inode(6, 600, 500).unwrap(),
        vec![0; 300]
    );
    assert_eq!(
        reader.read_file_range_by_inode(6, 880, 100).unwrap(),
        vec![0; 20]
    );
    assert!(reader
        .read_file_range_by_inode(6, 900, 1)
        .unwrap()
        .is_empty());
    assert!(reader
        .read_file_range_by_inode(6, u64::MAX, 1)
        .unwrap()
        .is_empty());
}

#[test]
fn streaming_reads_cross_valid_data_boundary_and_keep_logical_length() {
    let (reader, _directory) = filesystem(&image(900, 600, false));
    let mut stream = reader.into_file_stream_by_inode(6).unwrap();
    assert_eq!(stream.seek(SeekFrom::End(0)).unwrap(), 900);
    stream.seek(SeekFrom::Start(590)).unwrap();
    let mut bytes = [0xff; 40];
    stream.read_exact(&mut bytes).unwrap();
    assert_eq!(bytes, expected(900, 600)[590..630]);
    stream.seek(SeekFrom::Start(0)).unwrap();
    let mut all = Vec::new();
    stream.read_to_end(&mut all).unwrap();
    assert_eq!(all, expected(900, 600));
}

#[test]
fn continuation_extent_uses_initial_extent_valid_data_length() {
    for initialized in [300, 700, 1024] {
        let (reader, _directory) = filesystem(&image(1024, initialized, true));
        let mut bytes = Vec::new();
        reader
            .open_file("mft:6")
            .unwrap()
            .read_to_end(&mut bytes)
            .unwrap();
        let oracle = expected(1024, initialized as usize);
        assert_eq!(bytes, oracle);
        assert_eq!(
            reader.read_file_range_by_inode(6, 490, 250).unwrap(),
            oracle[490..740]
        );
        let mut stream = reader.into_file_stream_by_inode(6).unwrap();
        let mut all = Vec::new();
        stream.read_to_end(&mut all).unwrap();
        assert_eq!(all, oracle);
    }
}

#[test]
fn continuation_size_fields_do_not_override_stream_bounds() {
    let mut fixture = image(1024, 700, true);
    let continuation = ATTRIBUTE_OFFSET + 72;
    fixture[continuation + 40..continuation + 48].copy_from_slice(&1u64.to_le_bytes());
    fixture[continuation + 48..continuation + 56].copy_from_slice(&u64::MAX.to_le_bytes());
    fixture[continuation + 56..continuation + 64].copy_from_slice(&u64::MAX.to_le_bytes());
    let (reader, _directory) = filesystem(&fixture);
    let mut bytes = Vec::new();
    reader
        .open_file("mft:6")
        .unwrap()
        .read_to_end(&mut bytes)
        .unwrap();
    assert_eq!(bytes, expected(1024, 700));
    assert_eq!(
        reader.read_file_range_by_inode(6, 650, 100).unwrap(),
        bytes[650..750]
    );
}

#[test]
fn zero_valid_data_length_is_not_treated_as_missing_metadata() {
    let mut fixture = image(900, 0, false);
    // The physical allocation is unreadable; logical zero reads still succeed.
    fixture.truncate(DATA_OFFSET);
    let (reader, _directory) = filesystem(&fixture);
    let mut bytes = Vec::new();
    reader
        .open_file("mft:6")
        .unwrap()
        .read_to_end(&mut bytes)
        .unwrap();
    assert_eq!(bytes, vec![0; 900]);
    assert_eq!(reader.read_file_range_by_inode(6, 0, 900).unwrap(), bytes);
    let mut stream = reader.into_file_stream_by_inode(6).unwrap();
    let mut all = Vec::new();
    stream.read_to_end(&mut all).unwrap();
    assert_eq!(all, bytes);
}

#[test]
fn uninitialized_tail_is_not_read_from_the_evidence_source() {
    let mut fixture = image(900, 600, false);
    fixture.truncate(DATA_OFFSET + 600);
    let (reader, _directory) = filesystem(&fixture);
    let mut bytes = Vec::new();
    reader
        .open_file("mft:6")
        .unwrap()
        .read_to_end(&mut bytes)
        .unwrap();
    assert_eq!(bytes, expected(900, 600));
    assert_eq!(
        reader.read_file_range_by_inode(6, 590, 40).unwrap(),
        bytes[590..630]
    );
}

#[test]
fn fully_initialized_file_keeps_all_source_bytes() {
    let (reader, _directory) = filesystem(&image(900, 900, false));
    let mut bytes = Vec::new();
    reader
        .open_file("mft:6")
        .unwrap()
        .read_to_end(&mut bytes)
        .unwrap();
    assert_eq!(bytes, expected(900, 900));
}

#[test]
fn zero_sized_file_with_allocation_remains_empty() {
    let (reader, _directory) = filesystem(&image(0, 0, false));
    assert_eq!(reader.file_size_by_inode(6).unwrap(), Some(0));
    let mut bytes = Vec::new();
    reader
        .open_file("mft:6")
        .unwrap()
        .read_to_end(&mut bytes)
        .unwrap();
    assert!(bytes.is_empty());
    let mut stream = reader.into_file_stream_by_inode(6).unwrap();
    assert_eq!(stream.seek(SeekFrom::End(0)).unwrap(), 0);
    assert_eq!(stream.read(&mut [0; 1]).unwrap(), 0);
}

#[test]
fn invalid_initialized_size_is_rejected_on_all_logical_read_paths() {
    let (reader, _directory) = filesystem(&image(900, 901, false));
    assert_eq!(
        reader.open_file("mft:6").err().unwrap().kind(),
        std::io::ErrorKind::InvalidData
    );
    assert_eq!(
        reader.read_file_range_by_inode(6, 0, 1).unwrap_err().kind(),
        std::io::ErrorKind::InvalidData
    );
    assert_eq!(
        reader.into_file_stream_by_inode(6).err().unwrap().kind(),
        std::io::ErrorKind::InvalidData
    );
}

#[test]
fn missing_initialized_mapping_is_not_fabricated_as_zero_data() {
    let (reader, _directory) = filesystem(&image(1100, 1100, false));
    assert_eq!(
        reader.open_file("mft:6").err().unwrap().kind(),
        std::io::ErrorKind::InvalidData
    );
    assert_eq!(
        reader
            .read_file_range_by_inode(6, 1000, 100)
            .unwrap_err()
            .kind(),
        std::io::ErrorKind::InvalidData
    );
    let mut stream = reader.into_file_stream_by_inode(6).unwrap();
    assert_eq!(
        stream.read(&mut [0; 1]).unwrap_err().kind(),
        std::io::ErrorKind::InvalidData
    );
}

#[test]
fn named_nonresident_stream_obeys_its_own_valid_data_length() {
    let mut fixture = image(900, 600, false);
    fixture[ATTRIBUTE_OFFSET + 9] = 1;
    fixture[ATTRIBUTE_OFFSET + 10..ATTRIBUTE_OFFSET + 12].copy_from_slice(&68u16.to_le_bytes());
    fixture[ATTRIBUTE_OFFSET + 68..ATTRIBUTE_OFFSET + 70]
        .copy_from_slice(&(b'Z' as u16).to_le_bytes());
    let (reader, _directory) = filesystem(&fixture);
    assert_eq!(
        reader.read_ads_by_inode(6, "Z").unwrap(),
        expected(900, 600)
    );
}

#[test]
fn compressed_file_zeros_tail_after_decompression() {
    let mut fixture = image(6000, 5000, false);
    fixture[ATTRIBUTE_OFFSET + 12..ATTRIBUTE_OFFSET + 14].copy_from_slice(&1u16.to_le_bytes());
    fixture[ATTRIBUTE_OFFSET + 24..ATTRIBUTE_OFFSET + 32].copy_from_slice(&15u64.to_le_bytes());
    fixture[ATTRIBUTE_OFFSET + 34..ATTRIBUTE_OFFSET + 36].copy_from_slice(&4u16.to_le_bytes());
    fixture[ATTRIBUTE_OFFSET + 40..ATTRIBUTE_OFFSET + 48].copy_from_slice(&512u64.to_le_bytes());
    fixture[ATTRIBUTE_OFFSET + 64..ATTRIBUTE_OFFSET + 70]
        .copy_from_slice(&[0x11, 1, 32, 0x01, 15, 0]);
    fixture[DATA_OFFSET..DATA_OFFSET + 512].fill(0);
    // Two compressed LZNT1 subchunks expand twelve source bytes to 6000 'A' bytes.
    let compressed = [
        0x03, 0xb0, 0x02, b'A', 0xfc, 0x0f, 0x03, 0xb0, 0x02, b'A', 0x6c, 0x07,
    ];
    fixture[DATA_OFFSET..DATA_OFFSET + compressed.len()].copy_from_slice(&compressed);
    let (reader, _directory) = filesystem(&fixture);
    let mut bytes = Vec::new();
    reader
        .open_file("mft:6")
        .unwrap()
        .read_to_end(&mut bytes)
        .unwrap();
    let mut oracle = vec![0; 6000];
    oracle[..5000].fill(b'A');
    assert_eq!(bytes, oracle);
    // The original compressed bytes are still available through physical evidence reads.
    let mut source = reader.into_reader();
    source.seek(SeekFrom::Start(DATA_OFFSET as u64)).unwrap();
    let mut header = [0; 2];
    source.read_exact(&mut header).unwrap();
    assert_eq!(header, [0x03, 0xb0]);
}

#[test]
fn physical_evidence_reads_preserve_uninitialized_allocation_bytes() {
    let fixture = image(900, 600, false);
    let (reader, directory) = filesystem(&fixture);
    assert_eq!(
        reader.read_file_range_by_inode(6, 600, 10).unwrap(),
        vec![0; 10]
    );
    let mut source = reader.into_reader();
    source
        .seek(SeekFrom::Start((DATA_OFFSET + 600) as u64))
        .unwrap();
    let mut physical = [0; 10];
    source.read_exact(&mut physical).unwrap();
    assert_eq!(physical, [b'B'; 10]);
    assert_eq!(
        std::fs::read(directory.path().join("initialized-data.raw")).unwrap(),
        fixture
    );
}
