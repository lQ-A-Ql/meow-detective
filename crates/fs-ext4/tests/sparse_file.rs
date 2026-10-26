use evidence_core::{filesystem::FileSystemReader, image::raw_reader::RawImageReader};
use fs_ext4::Ext4Reader;
use std::io::Read;
use testing::builders::ext4::minimal_ext4_image;

const BLOCK: usize = 4096;

fn sparse_image(indexed: bool) -> Vec<u8> {
    let mut image = minimal_ext4_image();
    let inode = 2 * BLOCK + 2 * 256;
    image[inode + 4..inode + 8].copy_from_slice(&(5 * BLOCK as u32 + 13).to_le_bytes());
    let mut leaf = [0u8; 60];
    leaf[0..2].copy_from_slice(&0xf30au16.to_le_bytes());
    leaf[2..4].copy_from_slice(&2u16.to_le_bytes());
    leaf[4..6].copy_from_slice(&4u16.to_le_bytes());
    leaf[12..16].copy_from_slice(&1u32.to_le_bytes());
    leaf[16..18].copy_from_slice(&1u16.to_le_bytes());
    leaf[20..24].copy_from_slice(&4u32.to_le_bytes());
    leaf[24..28].copy_from_slice(&3u32.to_le_bytes());
    leaf[28..30].copy_from_slice(&0x8001u16.to_le_bytes());
    leaf[32..36].copy_from_slice(&6u32.to_le_bytes());
    if indexed {
        image[7 * BLOCK..7 * BLOCK + leaf.len()].copy_from_slice(&leaf);
        leaf[2..4].copy_from_slice(&1u16.to_le_bytes());
        leaf[6..8].copy_from_slice(&1u16.to_le_bytes());
        leaf[12..].fill(0);
        leaf[16..20].copy_from_slice(&7u32.to_le_bytes());
    }
    image[inode + 0x28..inode + 0x64].copy_from_slice(&leaf);
    image
}

fn verify_sparse_file(indexed: bool) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("sparse.raw");
    let image = sparse_image(indexed);
    std::fs::write(&path, &image).unwrap();
    let reader = Ext4Reader::open(Box::new(RawImageReader::open(&path).unwrap()), 0).unwrap();
    let mut expected = vec![0u8; 5 * BLOCK + 13];
    expected[BLOCK..2 * BLOCK].copy_from_slice(&image[4 * BLOCK..5 * BLOCK]);
    let mut full = Vec::new();
    reader
        .open_file("test.txt")
        .unwrap()
        .read_to_end(&mut full)
        .unwrap();
    assert_eq!(full, expected);
    let mut ranged = Vec::new();
    for offset in (0..expected.len()).step_by(997) {
        ranged.extend(
            reader
                .read_file_range("test.txt", offset as u64, 997)
                .unwrap(),
        );
    }
    assert_eq!(ranged, full);
    assert_eq!(std::fs::read(path).unwrap(), image);
}

#[test]
fn leaf_file_preserves_head_interior_tail_holes_and_unwritten_extents() {
    verify_sparse_file(false);
}

#[test]
fn indexed_file_preserves_head_interior_tail_holes_and_unwritten_extents() {
    verify_sparse_file(true);
}

#[test]
fn overlapping_extents_are_rejected_by_full_and_range_reads() {
    for indexed in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("overlap.raw");
        let mut image = sparse_image(indexed);
        let leaf = if indexed {
            7 * BLOCK
        } else {
            2 * BLOCK + 2 * 256 + 0x28
        };
        image[leaf + 24..leaf + 28].copy_from_slice(&1u32.to_le_bytes());
        std::fs::write(&path, &image).unwrap();
        let reader = Ext4Reader::open(Box::new(RawImageReader::open(&path).unwrap()), 0).unwrap();
        let error = reader.open_file("test.txt").err().unwrap();
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
        let range_error = reader
            .read_file_range("test.txt", 0, 5 * BLOCK + 13)
            .unwrap_err();
        assert_eq!(range_error.kind(), std::io::ErrorKind::InvalidData);
        assert_eq!(
            range_error.to_string(),
            "overlapping or unordered ext4 extents"
        );
        assert_eq!(std::fs::read(path).unwrap(), image);
    }
}
