use std::io::{self, Read, Seek, SeekFrom};
use std::path::PathBuf;

use evidence_core::{EvidenceReader, FileSystemReader, ReaderInfo};
use fs_f2fs::{F2fsError, F2fsReader, SuperblockCopy};
use testing::builders::f2fs::minimal_f2fs_image;

const BLOCK_SIZE: usize = 4096;
const F2FS_MAGIC: u32 = 0xf2f5_2010;
const SUPERBLOCK_FEATURE_CHECKSUM: u32 = 0x0000_0800;
const SUPERBLOCK_FEATURE_OFFSET: usize = 2180;
const SUPERBLOCK_CHECKSUM_OFFSET: usize = 3068;
const CHECKPOINT_FLAGS_OFFSET: usize = 132;
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
                path: PathBuf::from("minimal.f2fs"),
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
fn lists_root_and_reads_seekable_file_ranges() {
    let reader = F2fsReader::open(Box::new(MemoryReader::new(minimal_f2fs_image())), 0)
        .expect("open F2FS fixture");
    assert_eq!(reader.superblock().source_copy, SuperblockCopy::Primary);
    let children = reader.list_children("").expect("list root");
    assert_eq!(children.len(), 1);
    assert_eq!(children[0].name, "hello.txt");
    assert_eq!(children[0].size, 10);
    assert_eq!(
        reader
            .read_file_range("hello.txt", 6, 5)
            .expect("read range"),
        b"F2FS"
    );

    let mut file = reader
        .open_file_seekable("hello.txt")
        .expect("open seekable file");
    file.seek(SeekFrom::Start(6)).expect("seek file");
    let mut suffix = String::new();
    file.read_to_string(&mut suffix).expect("read suffix");
    assert_eq!(suffix, "F2FS");
}

#[test]
fn falls_back_to_backup_superblock_and_rejects_checkpoint_corruption() {
    let mut backup = minimal_f2fs_image();
    backup[1024] = 0;
    let reader =
        F2fsReader::open(Box::new(MemoryReader::new(backup)), 0).expect("use backup superblock");
    assert_eq!(reader.superblock().source_copy, SuperblockCopy::Backup);

    let mut corrupt = minimal_f2fs_image();
    for block in [512usize, 519, 1024, 1031] {
        corrupt[block * 4096 + 4092] ^= 0xff;
    }
    let error = F2fsReader::open(Box::new(MemoryReader::new(corrupt)), 0)
        .err()
        .expect("reject both checkpoint packs");
    assert!(matches!(error, F2fsError::Invalid(_)));
}

#[test]
fn rejects_truncated_source_and_indirect_file_without_partial_results() {
    let mut truncated = minimal_f2fs_image();
    truncated.truncate(truncated.len() - 4096);
    let error = F2fsReader::open(Box::new(MemoryReader::new(truncated)), 0)
        .err()
        .expect("reject truncated source");
    assert!(matches!(error, F2fsError::Invalid(_)));

    let mut indirect = minimal_f2fs_image();
    let file_inode = 4097 * 4096;
    let size = 924u64 * 4096;
    indirect[file_inode + 16..file_inode + 24].copy_from_slice(&size.to_le_bytes());
    let reader = F2fsReader::open(Box::new(MemoryReader::new(indirect)), 0).expect("open metadata");
    let error = reader
        .open_file("hello.txt")
        .err()
        .expect("indirect node lookup is explicit unsupported");
    assert_eq!(error.kind(), io::ErrorKind::Unsupported);
}

#[test]
fn checkpoint_nat_journal_overrides_stale_nat_blocks() {
    let mut image = minimal_f2fs_image();
    let nat_entry = 2560 * 4096 + 4 * 9;
    image[nat_entry + 5..nat_entry + 9].fill(0);

    let journal = 1025 * 4096 + 3584;
    image[journal..journal + 2].copy_from_slice(&1u16.to_le_bytes());
    image[journal + 2..journal + 6].copy_from_slice(&4u32.to_le_bytes());
    image[journal + 7..journal + 11].copy_from_slice(&4u32.to_le_bytes());
    image[journal + 11..journal + 15].copy_from_slice(&4097u32.to_le_bytes());

    let reader = F2fsReader::open(Box::new(MemoryReader::new(image)), 0)
        .expect("NAT journal restores file inode mapping");
    assert_eq!(
        reader
            .read_file_range("hello.txt", 0, 10)
            .expect("read journal-mapped file"),
        b"Hello F2FS"
    );
}

#[test]
fn validates_superblock_checksums_and_rejects_corrupt_copies() {
    let mut image = minimal_f2fs_image();
    enable_superblock_checksums(&mut image);
    F2fsReader::open(Box::new(MemoryReader::new(image.clone())), 0)
        .expect("accept valid checksummed superblocks");

    for offset in [1024usize, BLOCK_SIZE + 1024] {
        image[offset + 100] ^= 0xff;
    }
    let error = F2fsReader::open(Box::new(MemoryReader::new(image)), 0)
        .err()
        .expect("reject corrupt checksummed superblocks");
    assert!(matches!(error, F2fsError::Invalid(_)));
}

#[test]
fn reports_encrypted_file_metadata_but_rejects_plaintext_reads() {
    let mut image = minimal_f2fs_image();
    let flags = 4097 * BLOCK_SIZE + 80;
    image[flags..flags + 4].copy_from_slice(&0x0000_0800u32.to_le_bytes());

    let reader = F2fsReader::open(Box::new(MemoryReader::new(image)), 0)
        .expect("open encrypted inode metadata");
    let children = reader.list_children("").expect("list encrypted file");
    assert_eq!(children.len(), 1);
    assert!(children[0].encrypted);
    let error = reader
        .open_file("hello.txt")
        .err()
        .expect("encrypted plaintext read must be rejected");
    assert_eq!(error.kind(), io::ErrorKind::Unsupported);
}

#[test]
fn selects_nat_secondary_copy_from_checkpoint_bitmap() {
    let mut image = minimal_f2fs_image();
    let primary_nat = 2560 * BLOCK_SIZE;
    let secondary_nat = 3072 * BLOCK_SIZE;
    image.copy_within(primary_nat..primary_nat + BLOCK_SIZE, secondary_nat);
    image[primary_nat..primary_nat + BLOCK_SIZE].fill(0);

    let checkpoint = 1024 * BLOCK_SIZE;
    image[checkpoint + CHECKPOINT_BITMAP_OFFSET] = 0x80;
    refresh_checkpoint_checksum(&mut image, 1024);

    let reader = F2fsReader::open(Box::new(MemoryReader::new(image)), 0)
        .expect("open with secondary NAT copy");
    assert_eq!(
        reader
            .read_file_range("hello.txt", 0, 10)
            .expect("read through secondary NAT"),
        b"Hello F2FS"
    );
}

#[test]
fn reads_nat_journal_from_compact_checkpoint_summary() {
    let mut image = minimal_f2fs_image();
    let nat_entry = 2560 * BLOCK_SIZE + 4 * 9;
    image[nat_entry + 5..nat_entry + 9].fill(0);

    let checkpoint = 1024 * BLOCK_SIZE;
    image[checkpoint + CHECKPOINT_FLAGS_OFFSET..checkpoint + CHECKPOINT_FLAGS_OFFSET + 4]
        .copy_from_slice(&0x0000_0004u32.to_le_bytes());
    refresh_checkpoint_checksum(&mut image, 1024);

    let journal = 1025 * BLOCK_SIZE;
    image[journal..journal + 2].copy_from_slice(&1u16.to_le_bytes());
    image[journal + 2..journal + 6].copy_from_slice(&4u32.to_le_bytes());
    image[journal + 7..journal + 11].copy_from_slice(&4u32.to_le_bytes());
    image[journal + 11..journal + 15].copy_from_slice(&4097u32.to_le_bytes());

    let reader = F2fsReader::open(Box::new(MemoryReader::new(image)), 0)
        .expect("compact NAT journal restores inode mapping");
    assert_eq!(
        reader
            .read_file_range("hello.txt", 0, 10)
            .expect("read compact-journal mapped file"),
        b"Hello F2FS"
    );
}

#[test]
fn preserves_unsupported_errors_when_all_metadata_copies_are_valid() {
    let mut superblocks = minimal_f2fs_image();
    for offset in [1024usize, BLOCK_SIZE + 1024] {
        superblocks[offset + 1664..offset + 1668].copy_from_slice(&1u32.to_le_bytes());
    }
    let error = F2fsReader::open(Box::new(MemoryReader::new(superblocks)), 0)
        .err()
        .expect("checkpoint payload remains unsupported");
    assert!(matches!(error, F2fsError::Unsupported(_)));

    let mut checkpoints = minimal_f2fs_image();
    for block in [512usize, 1024] {
        let offset = block * BLOCK_SIZE + CHECKPOINT_FLAGS_OFFSET;
        checkpoints[offset..offset + 4].copy_from_slice(&0x0000_0401u32.to_le_bytes());
        refresh_checkpoint_checksum(&mut checkpoints, block);
    }
    let error = F2fsReader::open(Box::new(MemoryReader::new(checkpoints)), 0)
        .err()
        .expect("large NAT bitmap remains unsupported");
    assert!(matches!(error, F2fsError::Unsupported(_)));
}

fn enable_superblock_checksums(image: &mut [u8]) {
    for offset in [1024usize, BLOCK_SIZE + 1024] {
        image[offset + SUPERBLOCK_FEATURE_OFFSET..offset + SUPERBLOCK_FEATURE_OFFSET + 4]
            .copy_from_slice(&SUPERBLOCK_FEATURE_CHECKSUM.to_le_bytes());
        let checksum = f2fs_crc32(
            F2FS_MAGIC,
            &image[offset..offset + SUPERBLOCK_CHECKSUM_OFFSET],
        );
        image[offset + SUPERBLOCK_CHECKSUM_OFFSET..offset + SUPERBLOCK_CHECKSUM_OFFSET + 4]
            .copy_from_slice(&checksum.to_le_bytes());
    }
}

fn refresh_checkpoint_checksum(image: &mut [u8], block: usize) {
    let offset = block * BLOCK_SIZE;
    let checksum = f2fs_crc32(
        F2FS_MAGIC,
        &image[offset..offset + CHECKPOINT_CHECKSUM_OFFSET],
    );
    image[offset + CHECKPOINT_CHECKSUM_OFFSET..offset + BLOCK_SIZE]
        .copy_from_slice(&checksum.to_le_bytes());
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
