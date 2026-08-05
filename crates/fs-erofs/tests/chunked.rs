use std::io::{self, Read, Seek, SeekFrom};
use std::path::PathBuf;

use evidence_core::{EvidenceReader, FileSystemReader, ReaderInfo};
use fs_erofs::ErofsReader;
use testing::builders::erofs::minimal_erofs_image;

const BLOCK_SIZE: usize = 4096;
const FILE_INODE: usize = 2 * BLOCK_SIZE + 2 * 32;
const FILE_INDEX: usize = FILE_INODE + 32;
const SUPERBLOCK_INCOMPAT: usize = 1024 + 80;

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
                path: PathBuf::from("chunked.erofs"),
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
fn reads_block_array_chunks_and_zero_fills_holes() {
    let mut image = chunked_image(0, 3 * BLOCK_SIZE as u64);
    write_u32(&mut image, FILE_INDEX, 4);
    write_u32(&mut image, FILE_INDEX + 4, u32::MAX);
    write_u32(&mut image, FILE_INDEX + 8, 5);
    image[4 * BLOCK_SIZE..5 * BLOCK_SIZE].fill(b'A');
    image[5 * BLOCK_SIZE..6 * BLOCK_SIZE].fill(b'B');

    let reader = open(image);
    let bytes = reader
        .read_file_range("hello.txt", BLOCK_SIZE as u64 - 2, BLOCK_SIZE + 4)
        .expect("read mapped, sparse, and mapped chunks");
    assert_eq!(&bytes[..2], b"AA");
    assert!(bytes[2..BLOCK_SIZE + 2].iter().all(|byte| *byte == 0));
    assert_eq!(&bytes[BLOCK_SIZE + 2..], b"BB");
}

#[test]
fn reads_wide_indexed_chunks_and_larger_chunk_sizes() {
    let mut image = chunked_image(0x0061, 6000);
    write_chunk_index(&mut image, FILE_INDEX, 4, 0);
    image[4 * BLOCK_SIZE..5 * BLOCK_SIZE].fill(b'C');
    image[5 * BLOCK_SIZE..6 * BLOCK_SIZE].fill(b'D');

    let reader = open(image);
    assert_eq!(
        reader
            .read_file_range("hello.txt", BLOCK_SIZE as u64 - 2, 4)
            .expect("read within an 8 KiB chunk"),
        b"CCDD"
    );
}

#[test]
fn rejects_external_chunk_devices_and_missing_feature_flags() {
    let mut external = chunked_image(0x0020, 10);
    write_chunk_index(&mut external, FILE_INDEX, 4, 1);
    let error = open(external)
        .read_file_range("hello.txt", 0, 1)
        .expect_err("external devices remain unsupported");
    assert_eq!(error.kind(), io::ErrorKind::Unsupported);

    let mut missing_feature = chunked_image(0, 10);
    write_u32(&mut missing_feature, SUPERBLOCK_INCOMPAT, 0);
    write_u32(&mut missing_feature, FILE_INDEX, 4);
    let error = match open(missing_feature).open_file("hello.txt") {
        Ok(_) => panic!("chunk inode requires the superblock capability"),
        Err(error) => error,
    };
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
}

fn chunked_image(format: u16, size: u64) -> Vec<u8> {
    let mut image = minimal_erofs_image();
    write_u32(&mut image, SUPERBLOCK_INCOMPAT, 0x0000_0004);
    write_u16(&mut image, FILE_INODE, 8);
    write_u32(&mut image, FILE_INODE + 8, size as u32);
    write_u16(&mut image, FILE_INODE + 16, format);
    write_u16(&mut image, FILE_INODE + 18, 0);
    image
}

fn open(image: Vec<u8>) -> ErofsReader {
    ErofsReader::open(Box::new(MemoryReader::new(image)), 0).expect("open chunked EROFS")
}

fn write_chunk_index(bytes: &mut [u8], offset: usize, block: u64, device: u16) {
    write_u16(bytes, offset, (block >> 32) as u16);
    write_u16(bytes, offset + 2, device);
    write_u32(bytes, offset + 4, block as u32);
}

fn write_u16(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn write_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}
