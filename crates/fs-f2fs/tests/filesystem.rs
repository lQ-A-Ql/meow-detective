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
const ROOT_INODE_BLOCK: usize = 4096;
const FILE_INODE_BLOCK: usize = 4097;
const INODE_ADDRESS_OFFSET: usize = 360;
const INLINE_DATA: u8 = 0x02;
const INLINE_DENTRY: u8 = 0x04;
const INLINE_XATTR: u8 = 0x01;
const EXTRA_ATTR: u8 = 0x20;

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
fn rejects_truncated_source_and_file_tree_beyond_format_capacity() {
    let mut truncated = minimal_f2fs_image();
    truncated.truncate(truncated.len() - 4096);
    let error = F2fsReader::open(Box::new(MemoryReader::new(truncated)), 0)
        .err()
        .expect("reject truncated source");
    assert!(matches!(error, F2fsError::Invalid(_)));

    let mut oversized = minimal_f2fs_image();
    let file_inode = 4097 * 4096;
    let values_per_node = 1018usize;
    let max_blocks = 923
        + 2 * values_per_node
        + 2 * values_per_node * values_per_node
        + values_per_node * values_per_node * values_per_node;
    let size = (max_blocks as u64 + 1) * BLOCK_SIZE as u64;
    oversized[file_inode + 16..file_inode + 24].copy_from_slice(&size.to_le_bytes());
    let reader =
        F2fsReader::open(Box::new(MemoryReader::new(oversized)), 0).expect("open metadata");
    let error = reader
        .open_file("hello.txt")
        .err()
        .expect("file tree beyond double-indirect capacity is unsupported");
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
fn reads_and_seeks_inline_regular_file_data() {
    let image = with_inline_file(minimal_f2fs_image(), 0, 0);
    let reader =
        F2fsReader::open(Box::new(MemoryReader::new(image)), 0).expect("open inline-file fixture");

    assert_eq!(
        reader
            .read_file_range("hello.txt", 6, 5)
            .expect("read inline range"),
        b"F2FS"
    );
    let mut file = reader
        .open_file_seekable("hello.txt")
        .expect("open inline seekable file");
    file.seek(SeekFrom::End(-4)).expect("seek inline file");
    let mut suffix = String::new();
    file.read_to_string(&mut suffix)
        .expect("read inline suffix");
    assert_eq!(suffix, "F2FS");
}

#[test]
fn lists_inline_directory_entries() {
    let mut image = minimal_f2fs_image();
    let inode_start = ROOT_INODE_BLOCK * BLOCK_SIZE;
    image[inode_start + 3] = INLINE_DENTRY;
    let inline_start = inode_start + INODE_ADDRESS_OFFSET + 4;
    let inline_end = inode_start + 4052;
    write_inline_directory(&mut image[inline_start..inline_end]);

    let reader = F2fsReader::open(Box::new(MemoryReader::new(image)), 0)
        .expect("open inline-directory fixture");
    let children = reader.list_children("").expect("list inline root");
    assert_eq!(children.len(), 1);
    assert_eq!(children[0].name, "hello.txt");
    assert_eq!(
        reader
            .read_file_range("hello.txt", 0, 10)
            .expect("read child of inline directory"),
        b"Hello F2FS"
    );
}

#[test]
fn adjusts_inline_capacity_for_extra_attributes_and_xattrs() {
    let image = with_inline_file(minimal_f2fs_image(), 16, 64);
    let reader = F2fsReader::open(Box::new(MemoryReader::new(image)), 0)
        .expect("open xattr-adjusted inline fixture");
    assert_eq!(
        reader
            .read_file_range("hello.txt", 0, 10)
            .expect("read adjusted inline data"),
        b"Hello F2FS"
    );
}

#[test]
fn rejects_oversized_and_underflowing_inline_metadata() {
    let mut oversized = with_inline_file(minimal_f2fs_image(), 0, 0);
    let inode_start = FILE_INODE_BLOCK * BLOCK_SIZE;
    let capacity = 3688u64;
    oversized[inode_start + 16..inode_start + 24].copy_from_slice(&(capacity + 1).to_le_bytes());
    let reader = F2fsReader::open(Box::new(MemoryReader::new(oversized)), 0)
        .expect("open oversized inline metadata");
    let error = reader
        .open_file("hello.txt")
        .err()
        .expect("reject oversized inline file");
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);

    let mut underflow = minimal_f2fs_image();
    underflow[inode_start + 3] = INLINE_DATA | EXTRA_ATTR;
    underflow[inode_start + INODE_ADDRESS_OFFSET..inode_start + INODE_ADDRESS_OFFSET + 2]
        .copy_from_slice(&3692u16.to_le_bytes());
    let reader = F2fsReader::open(Box::new(MemoryReader::new(underflow)), 0)
        .expect("root metadata remains readable");
    let error = reader
        .list_children("")
        .expect_err("reject underflowing inline capacity");
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
}

#[test]
fn rejects_encrypted_inline_file_plaintext_reads() {
    let mut image = with_inline_file(minimal_f2fs_image(), 0, 0);
    let flags = FILE_INODE_BLOCK * BLOCK_SIZE + 80;
    image[flags..flags + 4].copy_from_slice(&0x0000_0800u32.to_le_bytes());

    let reader = F2fsReader::open(Box::new(MemoryReader::new(image)), 0)
        .expect("open encrypted inline metadata");
    let error = reader
        .open_file("hello.txt")
        .err()
        .expect("encrypted inline plaintext read must be rejected");
    assert_eq!(error.kind(), io::ErrorKind::Unsupported);
}

#[test]
fn rejects_compressed_inode_before_reading_encoded_blocks() {
    let mut image = minimal_f2fs_image();
    let flags = FILE_INODE_BLOCK * BLOCK_SIZE + 80;
    image[flags..flags + 4].copy_from_slice(&0x0000_0004u32.to_le_bytes());

    let reader = F2fsReader::open(Box::new(MemoryReader::new(image)), 0)
        .expect("open compressed inode metadata");
    let error = reader
        .read_file_range("hello.txt", 0, 10)
        .expect_err("compressed bytes must not be exposed as plaintext");
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

fn with_inline_file(mut image: Vec<u8>, extra_size: u16, xattr_words: u16) -> Vec<u8> {
    let inode_start = FILE_INODE_BLOCK * BLOCK_SIZE;
    let mut inline_flags = INLINE_DATA;
    if extra_size != 0 {
        inline_flags |= EXTRA_ATTR;
    }
    if xattr_words != 0 {
        inline_flags |= INLINE_XATTR;
    }
    image[inode_start + 3] = inline_flags;
    image[inode_start + INODE_ADDRESS_OFFSET..inode_start + INODE_ADDRESS_OFFSET + 2]
        .copy_from_slice(&extra_size.to_le_bytes());
    image[inode_start + INODE_ADDRESS_OFFSET + 2..inode_start + INODE_ADDRESS_OFFSET + 4]
        .copy_from_slice(&xattr_words.to_le_bytes());
    let data_start = inode_start + INODE_ADDRESS_OFFSET + usize::from(extra_size) + 4;
    image[data_start..data_start + 10].copy_from_slice(b"Hello F2FS");
    image
}

fn write_inline_directory(bytes: &mut [u8]) {
    let entry_count = bytes.len() * 8 / 153;
    let bitmap_bytes = entry_count.div_ceil(8);
    let reserved_bytes = bytes.len() - bitmap_bytes - entry_count * 19;
    let table_offset = bitmap_bytes + reserved_bytes;
    let name_offset = table_offset + entry_count * 11;
    bytes[0] = 0x0f;
    write_inline_dentry(bytes, table_offset, name_offset, 0, 3, 2, ".");
    write_inline_dentry(bytes, table_offset, name_offset, 1, 3, 2, "..");
    write_inline_dentry(bytes, table_offset, name_offset, 2, 4, 1, "hello.txt");
}

fn write_inline_dentry(
    bytes: &mut [u8],
    table_offset: usize,
    name_offset: usize,
    slot: usize,
    inode: u32,
    file_type: u8,
    name: &str,
) {
    let entry_offset = table_offset + slot * 11;
    bytes[entry_offset + 4..entry_offset + 8].copy_from_slice(&inode.to_le_bytes());
    bytes[entry_offset + 8..entry_offset + 10].copy_from_slice(&(name.len() as u16).to_le_bytes());
    bytes[entry_offset + 10] = file_type;
    let name_start = name_offset + slot * 8;
    bytes[name_start..name_start + name.len()].copy_from_slice(name.as_bytes());
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
