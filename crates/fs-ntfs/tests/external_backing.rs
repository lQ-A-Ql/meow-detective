use evidence_core::{filesystem::FileSystemReader, image::raw_reader::RawImageReader};
use fs_ntfs::NtfsReader;
use std::io;

fn resident(record: &mut [u8], position: usize, kind: u32, content: &[u8]) -> usize {
    let length = (24 + content.len() + 7) & !7;
    record[position..position + 4].copy_from_slice(&kind.to_le_bytes());
    record[position + 4..position + 8].copy_from_slice(&(length as u32).to_le_bytes());
    record[position + 16..position + 20].copy_from_slice(&(content.len() as u32).to_le_bytes());
    record[position + 20..position + 22].copy_from_slice(&24u16.to_le_bytes());
    record[position + 24..position + 24 + content.len()].copy_from_slice(content);
    position + length
}

fn filesystem(tag: Option<&[u8]>) -> (NtfsReader, tempfile::TempDir) {
    let temporary = tempfile::tempdir().unwrap();
    let mut image = vec![0; 10 * 1024];
    image[3..11].copy_from_slice(b"NTFS    ");
    image[11..13].copy_from_slice(&512u16.to_le_bytes());
    image[13] = 1;
    image[0x30..0x38].copy_from_slice(&2u64.to_le_bytes());
    image[0x40] = (-10i8) as u8;
    let record = &mut image[1024 + 6 * 1024..1024 + 7 * 1024];
    record[..4].copy_from_slice(b"FILE");
    record[0x14..0x16].copy_from_slice(&0x38u16.to_le_bytes());
    let mut position = resident(record, 0x38, 0x80, b"MZ content");
    if let Some(tag) = tag {
        position = resident(record, position, 0xc0, tag);
    }
    record[position..position + 4].copy_from_slice(&u32::MAX.to_le_bytes());
    let path = temporary.path().join("ntfs.raw");
    std::fs::write(&path, image).unwrap();
    (
        NtfsReader::open(Box::new(RawImageReader::open(&path).unwrap()), 0).unwrap(),
        temporary,
    )
}

#[test]
fn truncated_wof_header_is_not_treated_as_readable_content() {
    let (fs, _temporary) = filesystem(Some(&0x8000_0017u32.to_le_bytes()));
    assert_eq!(fs.file_size_by_inode(6).unwrap(), Some(10));
    assert_eq!(
        fs.read_file_range_by_inode(6, 0, 2).unwrap_err().kind(),
        io::ErrorKind::InvalidData
    );
    assert_eq!(
        fs.open_file("mft:6").err().unwrap().kind(),
        io::ErrorKind::InvalidData
    );
    assert!(fs.supports_file_stream_by_inode(6).is_err());
    assert_eq!(
        fs.into_file_stream_by_inode(6).err().unwrap().kind(),
        io::ErrorKind::InvalidData
    );
}

#[test]
fn ordinary_files_and_other_reparse_tags_preserve_readable_content() {
    for tag in [None, Some(0xa000_000cu32.to_le_bytes())] {
        let (fs, _temporary) = filesystem(tag.as_ref().map(|tag| tag.as_slice()));
        assert_eq!(fs.read_file_range_by_inode(6, 0, 2).unwrap(), b"MZ");
        assert!(fs.supports_file_stream_by_inode(6).unwrap());
    }
}

#[test]
fn truncated_reparse_data_returns_an_integrity_error() {
    let (fs, _temporary) = filesystem(Some(&[0x17, 0, 0]));
    assert_eq!(
        fs.read_file_range_by_inode(6, 0, 2).unwrap_err().kind(),
        io::ErrorKind::InvalidData
    );
}
