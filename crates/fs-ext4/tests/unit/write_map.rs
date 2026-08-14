use super::*;

use std::io::{Read, Seek, SeekFrom};

use evidence_core::ReaderInfo;

struct FakeReader {
    data: Vec<u8>,
    pos: u64,
    info: ReaderInfo,
}

impl FakeReader {
    fn new(data: Vec<u8>) -> Self {
        let size = data.len() as u64;
        Self {
            data,
            pos: 0,
            info: ReaderInfo {
                path: std::path::PathBuf::from("fake-ext4"),
                size,
                kind: "fake-ext4".to_string(),
            },
        }
    }
}

impl Read for FakeReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let start = (self.pos as usize).min(self.data.len());
        let end = (start + buf.len()).min(self.data.len());
        let n = end - start;
        buf[..n].copy_from_slice(&self.data[start..end]);
        self.pos += n as u64;
        Ok(n)
    }
}

impl Seek for FakeReader {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        self.pos = match pos {
            SeekFrom::Start(p) => p,
            SeekFrom::End(p) => (self.data.len() as i64 + p).max(0) as u64,
            SeekFrom::Current(p) => (self.pos as i64 + p).max(0) as u64,
        };
        Ok(self.pos)
    }
}

impl evidence_core::EvidenceReader for FakeReader {
    fn info(&self) -> &ReaderInfo {
        &self.info
    }
}

fn open(image: Vec<u8>) -> crate::Ext4Reader {
    let reader: Box<dyn evidence_core::EvidenceReader> = Box::new(FakeReader::new(image));
    crate::Ext4Reader::open(reader, 0).unwrap()
}

#[test]
fn maps_a_regular_file_to_its_physical_block() {
    let fs = open(testing::builders::ext4::linux_root_ext4_image());
    let extents = fs.file_extent_map("etc/os-release").unwrap();
    assert_eq!(extents.len(), 1);
    assert_eq!(extents[0].logical_offset, 0);
    assert_eq!(extents[0].volume_offset, 5 * 4096);
    assert_eq!(extents[0].length, 4096);
    let expected_size =
        b"NAME=\"CentOS Linux\"\nID=\"centos\"\nPRETTY_NAME=\"CentOS Linux 7 (Core)\"\n".len()
            as u64;
    assert_eq!(
        fs.file_size_by_path("etc/os-release").unwrap(),
        expected_size
    );
}

#[test]
fn refuses_directories_and_missing_files() {
    let fs = open(testing::builders::ext4::linux_root_ext4_image());
    assert!(fs.file_extent_map("etc").is_err());
    assert!(fs.file_extent_map("etc/nope").is_err());
}

#[test]
fn refuses_files_without_the_extents_flag() {
    let mut image = testing::builders::ext4::linux_root_ext4_image();
    // Clear EXT4_EXTENTS_FL on the shadow inode (inode 10, i_flags at 0x20).
    let inode = 2 * 4096 + 9 * 256;
    image[inode + 0x20..inode + 0x24].copy_from_slice(&0u32.to_le_bytes());
    let fs = open(image);
    let error = fs.file_extent_map("etc/shadow").unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
}

const SHADOW_INODE: usize = 2 * 4096 + 9 * 256; // inode 10 in linux_root images
const LEAF_BLOCK: usize = 15 * 4096; // unused trailing block in linux_root images

/// Rewrites the shadow inode as a depth-1 extent tree with `entries` index
/// records, all pointing at physical block 15.
fn make_depth1_shadow_tree(image: &mut [u8], entries: u16) {
    image[SHADOW_INODE + 0x2A..SHADOW_INODE + 0x2C].copy_from_slice(&entries.to_le_bytes());
    image[SHADOW_INODE + 0x2E..SHADOW_INODE + 0x30].copy_from_slice(&1u16.to_le_bytes());
    for index in 0..entries as usize {
        let entry = SHADOW_INODE + 0x34 + index * 12;
        image[entry..entry + 4].copy_from_slice(&(index as u32).to_le_bytes()); // ei_block
        image[entry + 4..entry + 8].copy_from_slice(&15u32.to_le_bytes()); // ei_leaf_lo
        image[entry + 8..entry + 10].copy_from_slice(&0u16.to_le_bytes()); // ei_leaf_hi
    }
}

/// Writes an extent-tree node header at block 15 with the given depth.
fn make_leaf_block(image: &mut [u8], depth: u16) {
    image[LEAF_BLOCK..LEAF_BLOCK + 2].copy_from_slice(&0xF30Au16.to_le_bytes());
    image[LEAF_BLOCK + 2..LEAF_BLOCK + 4].copy_from_slice(&1u16.to_le_bytes());
    image[LEAF_BLOCK + 4..LEAF_BLOCK + 6].copy_from_slice(&4u16.to_le_bytes());
    image[LEAF_BLOCK + 6..LEAF_BLOCK + 8].copy_from_slice(&depth.to_le_bytes());
    // One extent/index record: logical 0 -> physical block 11.
    image[LEAF_BLOCK + 16..LEAF_BLOCK + 18].copy_from_slice(&1u16.to_le_bytes());
    image[LEAF_BLOCK + 20..LEAF_BLOCK + 24].copy_from_slice(&11u32.to_le_bytes());
}

#[test]
fn rejects_duplicate_extent_index_blocks() {
    let mut image = testing::builders::ext4::linux_root_ext4_image();
    make_depth1_shadow_tree(&mut image, 2);
    make_leaf_block(&mut image, 0);
    let fs = open(image);
    let error = fs.file_extent_map("etc/shadow").unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    assert!(error.to_string().contains("more than once"));
}

#[test]
fn rejects_extent_tree_depth_mismatch() {
    let mut image = testing::builders::ext4::linux_root_ext4_image();
    make_depth1_shadow_tree(&mut image, 1);
    // The child claims depth 1 where the tree expects a leaf (depth 0).
    make_leaf_block(&mut image, 1);
    let fs = open(image);
    let error = fs.file_extent_map("etc/shadow").unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    assert!(error.to_string().contains("depth"));
}

#[test]
fn rejects_unordered_leaf_extents() {
    let mut image = testing::builders::ext4::linux_root_ext4_image();
    // Two leaf extents whose ee_block values run backwards (5 then 2).
    image[SHADOW_INODE + 0x2A..SHADOW_INODE + 0x2C].copy_from_slice(&2u16.to_le_bytes());
    image[SHADOW_INODE + 0x34..SHADOW_INODE + 0x38].copy_from_slice(&5u32.to_le_bytes());
    let second = SHADOW_INODE + 0x40;
    image[second..second + 4].copy_from_slice(&2u32.to_le_bytes()); // ee_block
    image[second + 4..second + 6].copy_from_slice(&1u16.to_le_bytes()); // ee_len
    image[second + 6..second + 8].copy_from_slice(&0u16.to_le_bytes()); // ee_start_hi
    image[second + 8..second + 12].copy_from_slice(&12u32.to_le_bytes()); // ee_start_lo
    let fs = open(image);
    let error = fs.file_extent_map("etc/shadow").unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    assert!(error.to_string().contains("not ordered"));
}
