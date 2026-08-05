use std::io::{self, Read, Seek, SeekFrom};
use std::path::PathBuf;

use evidence_core::{EvidenceReader, FileSystemReader, ReaderInfo};
use fs_f2fs::{F2fsError, F2fsReader};
use testing::builders::f2fs::minimal_f2fs_image;

const BLOCK_SIZE: usize = 4096;
const F2FS_MAGIC: u32 = 0xf2f5_2010;
const CHECKPOINT_FLAGS_OFFSET: usize = 132;
const CHECKPOINT_TOTAL_BLOCKS_OFFSET: usize = 136;
const CHECKPOINT_START_SUM_OFFSET: usize = 140;
const CHECKPOINT_SIT_SIZE_OFFSET: usize = 156;
const CHECKPOINT_NAT_SIZE_OFFSET: usize = 160;
const CHECKPOINT_CHECKSUM_FIELD_OFFSET: usize = 164;
const CHECKPOINT_BITMAP_OFFSET: usize = 192;
const CHECKPOINT_CHECKSUM_OFFSET: usize = BLOCK_SIZE - 4;

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
                path: PathBuf::from("checkpoint-payload.f2fs"),
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
fn reads_checkpoint_payload_and_large_nat_bitmap_layouts() {
    for large_nat in [false, true] {
        let image = with_checkpoint_payload(minimal_f2fs_image(), large_nat);
        let reader = F2fsReader::open(Box::new(MemoryReader::new(image)), 0)
            .expect("open checkpoint payload fixture");
        assert_eq!(
            reader
                .read_file_range("hello.txt", 0, 10)
                .expect("read through checkpoint payload"),
            b"Hello F2FS"
        );
    }
}

#[test]
fn reads_nat_journal_after_checkpoint_payload_blocks() {
    let mut image = with_checkpoint_payload(minimal_f2fs_image(), false);
    let nat_entry = 2560 * BLOCK_SIZE + 4 * 9;
    image[nat_entry + 5..nat_entry + 9].fill(0);
    let journal = (1024 + 2) * BLOCK_SIZE + 3584;
    image[journal..journal + 2].copy_from_slice(&1u16.to_le_bytes());
    image[journal + 2..journal + 6].copy_from_slice(&4u32.to_le_bytes());
    image[journal + 7..journal + 11].copy_from_slice(&4u32.to_le_bytes());
    image[journal + 11..journal + 15].copy_from_slice(&4097u32.to_le_bytes());

    let reader = F2fsReader::open(Box::new(MemoryReader::new(image)), 0)
        .expect("payload-aware NAT journal restores inode mapping");
    assert_eq!(
        reader
            .read_file_range("hello.txt", 0, 10)
            .expect("read journal-mapped file"),
        b"Hello F2FS"
    );
}

#[test]
fn rejects_checkpoint_payload_beyond_segment_capacity() {
    let mut image = minimal_f2fs_image();
    for offset in [1024usize, BLOCK_SIZE + 1024] {
        image[offset + 1664..offset + 1668].copy_from_slice(&504u32.to_le_bytes());
    }
    let error = F2fsReader::open(Box::new(MemoryReader::new(image)), 0)
        .err()
        .expect("reject insane checkpoint payload");
    assert!(matches!(error, F2fsError::Invalid(_)));
}

fn with_checkpoint_payload(mut image: Vec<u8>, large_nat: bool) -> Vec<u8> {
    for offset in [1024usize, BLOCK_SIZE + 1024] {
        image[offset + 1664..offset + 1668].copy_from_slice(&1u32.to_le_bytes());
    }
    for start in [512usize, 1024] {
        let old_tail = image[(start + 7) * BLOCK_SIZE..(start + 8) * BLOCK_SIZE].to_vec();
        image[(start + 8) * BLOCK_SIZE..(start + 9) * BLOCK_SIZE].copy_from_slice(&old_tail);
        for block in [start, start + 8] {
            configure_checkpoint_block(&mut image, block, large_nat);
        }
    }
    image
}

fn configure_checkpoint_block(image: &mut [u8], block: usize, large_nat: bool) {
    let offset = block * BLOCK_SIZE;
    let flags: u32 = if large_nat { 0x0000_0401 } else { 0x0000_0001 };
    write_u32(image, offset + CHECKPOINT_FLAGS_OFFSET, flags);
    write_u32(image, offset + CHECKPOINT_TOTAL_BLOCKS_OFFSET, 9);
    write_u32(image, offset + CHECKPOINT_START_SUM_OFFSET, 2);
    write_u32(
        image,
        offset + CHECKPOINT_SIT_SIZE_OFFSET,
        if large_nat { 16 } else { 1 },
    );
    write_u32(
        image,
        offset + CHECKPOINT_NAT_SIZE_OFFSET,
        if large_nat { 4000 } else { 1 },
    );
    let checksum_offset = if large_nat {
        CHECKPOINT_BITMAP_OFFSET
    } else {
        CHECKPOINT_CHECKSUM_OFFSET
    };
    write_u32(
        image,
        offset + CHECKPOINT_CHECKSUM_FIELD_OFFSET,
        checksum_offset as u32,
    );
    let checksum = f2fs_crc32(F2FS_MAGIC, &image[offset..offset + checksum_offset]);
    write_u32(image, offset + checksum_offset, checksum);
}

fn write_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn f2fs_crc32(mut crc: u32, bytes: &[u8]) -> u32 {
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320 & 0u32.wrapping_sub(crc & 1));
        }
    }
    crc
}
