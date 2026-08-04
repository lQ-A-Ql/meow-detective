use std::io::{self, Read, Seek, SeekFrom};
use std::path::PathBuf;

use evidence_core::{EvidenceReader, FileSystemReader, ReaderInfo};
use fs_f2fs::F2fsReader;
use testing::builders::f2fs::minimal_f2fs_image;

const BLOCK_SIZE: usize = 4096;
const NAT_BLOCK: usize = 2560;
const FILE_INODE_BLOCK: usize = 4097;
const VALUES_PER_NODE: usize = 1018;
const NODE_FOOTER_OFFSET: usize = 4072;

struct MemoryReader {
    bytes: Vec<u8>,
    cursor: u64,
    info: ReaderInfo,
}

impl MemoryReader {
    fn new(bytes: Vec<u8>) -> Self {
        let size = bytes.len() as u64;
        Self {
            bytes,
            cursor: 0,
            info: ReaderInfo {
                path: PathBuf::from("indirect.f2fs"),
                size,
                kind: "test-f2fs".to_string(),
            },
        }
    }
}

impl Read for MemoryReader {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        let start = usize::try_from(self.cursor)
            .unwrap_or(usize::MAX)
            .min(self.bytes.len());
        let end = start.saturating_add(output.len()).min(self.bytes.len());
        let length = end - start;
        output[..length].copy_from_slice(&self.bytes[start..end]);
        self.cursor += length as u64;
        Ok(length)
    }
}

impl Seek for MemoryReader {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        let next = match position {
            SeekFrom::Start(value) => i128::from(value),
            SeekFrom::End(value) => self.bytes.len() as i128 + i128::from(value),
            SeekFrom::Current(value) => i128::from(self.cursor) + i128::from(value),
        };
        if next < 0 || next > i128::from(u64::MAX) {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "invalid seek"));
        }
        self.cursor = next as u64;
        Ok(self.cursor)
    }
}

impl EvidenceReader for MemoryReader {
    fn info(&self) -> &ReaderInfo {
        &self.info
    }
}

#[test]
fn reads_ranges_through_direct_indirect_and_double_indirect_nodes() {
    let mut image = indirect_tree_image();
    let direct_one_index = 923usize;
    let direct_two_index = direct_one_index + VALUES_PER_NODE;
    let indirect_one_index = direct_two_index + VALUES_PER_NODE;
    let indirect_two_index = indirect_one_index + VALUES_PER_NODE * VALUES_PER_NODE;
    let double_index = indirect_two_index + VALUES_PER_NODE * VALUES_PER_NODE;
    let file_inode = FILE_INODE_BLOCK * BLOCK_SIZE;
    let file_size = double_index as u64 * BLOCK_SIZE as u64 + 8;
    image[file_inode + 16..file_inode + 24].copy_from_slice(&file_size.to_le_bytes());

    let reader = F2fsReader::open(Box::new(MemoryReader::new(image)), 0)
        .expect("open indirect-tree fixture");
    assert_range(&reader, direct_one_index, b"direct-1");
    assert_range(&reader, direct_two_index, b"direct-2");
    assert_range(&reader, indirect_one_index, b"indirec1");
    assert_range(&reader, indirect_two_index, b"indirec2");
    assert_range(&reader, double_index, b"double!!");

    let hole_index = direct_one_index + 1;
    assert_eq!(
        reader
            .read_file_range("hello.txt", hole_index as u64 * BLOCK_SIZE as u64, 8)
            .expect("read sparse missing node"),
        [0u8; 8]
    );
}

#[test]
fn rejects_mismatched_indirect_node_footer_on_first_access() {
    let mut image = indirect_tree_image();
    let file_inode = FILE_INODE_BLOCK * BLOCK_SIZE;
    let direct_index = 923usize;
    image[file_inode + 16..file_inode + 24]
        .copy_from_slice(&((direct_index as u64 + 1) * BLOCK_SIZE as u64).to_le_bytes());
    let footer = 4100 * BLOCK_SIZE + NODE_FOOTER_OFFSET;
    image[footer..footer + 4].copy_from_slice(&99u32.to_le_bytes());

    let reader = F2fsReader::open(Box::new(MemoryReader::new(image)), 0)
        .expect("unrelated metadata remains readable");
    assert_eq!(
        reader
            .read_file_range("hello.txt", direct_index as u64 * BLOCK_SIZE as u64, 8)
            .expect_err("reject mismatched node footer")
            .kind(),
        io::ErrorKind::InvalidData
    );
}

fn indirect_tree_image() -> Vec<u8> {
    let block_count = 4120usize;
    let mut image = minimal_f2fs_image();
    image.resize(block_count * BLOCK_SIZE, 0);
    for offset in [1024usize, BLOCK_SIZE + 1024] {
        image[offset + 36..offset + 44].copy_from_slice(&(block_count as u64).to_le_bytes());
    }
    let inode = FILE_INODE_BLOCK * BLOCK_SIZE;
    for (index, nid) in [5u32, 6, 7, 9, 11].into_iter().enumerate() {
        image[inode + 4052 + index * 4..inode + 4056 + index * 4]
            .copy_from_slice(&nid.to_le_bytes());
    }
    write_node(&mut image, 5, 4100, 4115);
    write_node(&mut image, 6, 4101, 4116);
    write_node(&mut image, 7, 4102, 8);
    write_node(&mut image, 8, 4103, 4117);
    write_node(&mut image, 9, 4104, 10);
    write_node(&mut image, 10, 4105, 4118);
    write_node(&mut image, 11, 4106, 12);
    write_node(&mut image, 12, 4107, 13);
    write_node(&mut image, 13, 4108, 4119);
    block_mut(&mut image, 4115)[..8].copy_from_slice(b"direct-1");
    block_mut(&mut image, 4116)[..8].copy_from_slice(b"direct-2");
    block_mut(&mut image, 4117)[..8].copy_from_slice(b"indirec1");
    block_mut(&mut image, 4118)[..8].copy_from_slice(b"indirec2");
    block_mut(&mut image, 4119)[..8].copy_from_slice(b"double!!");
    image
}

fn write_node(image: &mut [u8], nid: u32, block: usize, first_value: u32) {
    let nat_offset = nid as usize * 9;
    let nat = block_mut(image, NAT_BLOCK);
    nat[nat_offset + 1..nat_offset + 5].copy_from_slice(&4u32.to_le_bytes());
    nat[nat_offset + 5..nat_offset + 9].copy_from_slice(&(block as u32).to_le_bytes());
    let node = block_mut(image, block);
    node[..4].copy_from_slice(&first_value.to_le_bytes());
    node[NODE_FOOTER_OFFSET..NODE_FOOTER_OFFSET + 4].copy_from_slice(&nid.to_le_bytes());
    node[NODE_FOOTER_OFFSET + 4..NODE_FOOTER_OFFSET + 8].copy_from_slice(&4u32.to_le_bytes());
}

fn assert_range(reader: &F2fsReader, logical_block: usize, expected: &[u8]) {
    assert_eq!(
        reader
            .read_file_range(
                "hello.txt",
                logical_block as u64 * BLOCK_SIZE as u64,
                expected.len(),
            )
            .expect("read indirect range"),
        expected
    );
}

fn block_mut(image: &mut [u8], block: usize) -> &mut [u8] {
    &mut image[block * BLOCK_SIZE..(block + 1) * BLOCK_SIZE]
}
