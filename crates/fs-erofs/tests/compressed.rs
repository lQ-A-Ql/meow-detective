use std::io::{self, Read, Seek, SeekFrom};
use std::path::PathBuf;

use evidence_core::{EvidenceReader, FileSystemReader, ReaderInfo};
use fs_erofs::ErofsReader;
use testing::builders::erofs::minimal_erofs_image;

const BLOCK_SIZE: usize = 4096;
const FILE_INODE: usize = 2 * BLOCK_SIZE + 2 * 32;
const MAP_HEADER: usize = FILE_INODE + 32;
const FULL_INDEX: usize = MAP_HEADER + 16;
const SUPERBLOCK_INCOMPAT: usize = 1024 + 80;
const SUPERBLOCK_LZ4_MAX_DISTANCE: usize = 1024 + 84;

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
                path: PathBuf::from("compressed.erofs"),
                size,
                kind: "test-erofs".to_string(),
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
fn reads_lz4_and_plain_clusters_without_materializing_the_file() {
    let mut image = compressed_image(2 * BLOCK_SIZE as u32);
    let first = vec![b'L'; BLOCK_SIZE];
    let encoded = lz4_flex::block::compress(&first);
    let encoded_start = 7 * BLOCK_SIZE - encoded.len();
    image[encoded_start..encoded_start + encoded.len()].copy_from_slice(&encoded);
    image[7 * BLOCK_SIZE..8 * BLOCK_SIZE].fill(b'P');
    write_full_index(&mut image, FULL_INDEX, 1, 6);
    write_full_index(&mut image, FULL_INDEX + 8, 0, 7);

    let reader =
        ErofsReader::open(Box::new(MemoryReader::new(image)), 0).expect("open compressed EROFS");
    assert_eq!(
        reader
            .read_file_range("hello.txt", BLOCK_SIZE as u64 - 2, 4)
            .expect("read across LZ4 and plain clusters"),
        b"LLPP"
    );
    assert_eq!(
        reader
            .read_file_range("hello.txt", 17, 5)
            .expect("repeat a bounded LZ4 range"),
        b"LLLLL"
    );
}

#[test]
fn zero_fills_declared_compressed_holes_and_rejects_nonhead_extents() {
    let mut hole = compressed_image(BLOCK_SIZE as u32);
    write_full_index(&mut hole, FULL_INDEX, 0x4000, u32::MAX);
    let reader = ErofsReader::open(Box::new(MemoryReader::new(hole)), 0)
        .expect("open compressed hole fixture");
    assert_eq!(
        reader
            .read_file_range("hello.txt", 0, 8)
            .expect("read compressed hole"),
        [0u8; 8]
    );

    let mut nonhead = compressed_image(BLOCK_SIZE as u32);
    write_full_index(&mut nonhead, FULL_INDEX, 2, 6);
    let error = ErofsReader::open(Box::new(MemoryReader::new(nonhead)), 0)
        .expect("open nonhead fixture")
        .read_file_range("hello.txt", 0, 1)
        .expect_err("multi-cluster extents remain unsupported");
    assert_eq!(error.kind(), io::ErrorKind::Unsupported);
}

#[test]
fn rejects_legacy_lz4_without_structural_input_length() {
    let mut image = compressed_image(BLOCK_SIZE as u32);
    write_u32(&mut image, SUPERBLOCK_INCOMPAT, 0);
    write_u16(&mut image, SUPERBLOCK_LZ4_MAX_DISTANCE, 4096);
    write_full_index(&mut image, FULL_INDEX, 1, 6);

    let error = ErofsReader::open(Box::new(MemoryReader::new(image)), 0)
        .expect("open legacy EROFS")
        .read_file_range("hello.txt", 0, 1)
        .expect_err("legacy trailing-padding LZ4 remains unsupported");
    assert_eq!(error.kind(), io::ErrorKind::Unsupported);
}

#[test]
fn rejects_inline_tail_data_and_out_of_bounds_physical_blocks() {
    let mut inline = compressed_image(BLOCK_SIZE as u32);
    write_u16(&mut inline, MAP_HEADER + 2, 1);
    let error = ErofsReader::open(Box::new(MemoryReader::new(inline)), 0)
        .expect("open inline-tail fixture")
        .read_file_range("hello.txt", 0, 1)
        .expect_err("inline compressed tails remain unsupported");
    assert_eq!(error.kind(), io::ErrorKind::Unsupported);

    let mut outside = compressed_image(BLOCK_SIZE as u32);
    write_full_index(&mut outside, FULL_INDEX, 1, 16);
    let error = ErofsReader::open(Box::new(MemoryReader::new(outside)), 0)
        .expect("open out-of-bounds fixture")
        .read_file_range("hello.txt", 0, 1)
        .expect_err("physical blocks outside the filesystem are invalid");
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);

    let oversized = compressed_image(u32::MAX);
    let error = ErofsReader::open(Box::new(MemoryReader::new(oversized)), 0)
        .expect("open oversized-index fixture")
        .read_file_range("hello.txt", 0, 1)
        .expect_err("compression index tables must remain inside the filesystem");
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
}

fn compressed_image(size: u32) -> Vec<u8> {
    let mut image = minimal_erofs_image();
    write_u32(&mut image, SUPERBLOCK_INCOMPAT, 1);
    write_u16(&mut image, FILE_INODE, 2);
    write_u32(&mut image, FILE_INODE + 8, size);
    image[MAP_HEADER..MAP_HEADER + 8].fill(0);
    image
}

fn write_full_index(bytes: &mut [u8], offset: usize, advise: u16, block: u32) {
    write_u16(bytes, offset, advise);
    write_u16(bytes, offset + 2, 0);
    write_u32(bytes, offset + 4, block);
}

fn write_u16(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn write_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}
