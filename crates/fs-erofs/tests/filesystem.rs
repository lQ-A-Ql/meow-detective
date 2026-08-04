use std::io::{self, Read, Seek, SeekFrom};
use std::path::PathBuf;

use evidence_core::{EvidenceReader, FileSystemReader, ReaderInfo};
use fs_erofs::{ErofsError, ErofsReader};
use testing::builders::erofs::minimal_erofs_image;

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
                path: PathBuf::from("minimal.erofs"),
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
fn lists_root_and_reads_flat_plain_file_ranges() {
    let reader = ErofsReader::open(Box::new(MemoryReader::new(minimal_erofs_image())), 0)
        .expect("open EROFS fixture");
    let children = reader.list_children("").expect("list EROFS root");
    assert_eq!(children.len(), 1);
    assert_eq!(children[0].name, "hello.txt");
    assert_eq!(children[0].size, 12);
    assert_eq!(
        reader
            .read_file_range("hello.txt", 6, 6)
            .expect("read bounded EROFS range"),
        b"EROFS!"
    );

    let mut file = reader
        .open_file_seekable("hello.txt")
        .expect("open seekable EROFS file");
    file.seek(SeekFrom::Start(6)).expect("seek EROFS file");
    let mut suffix = String::new();
    file.read_to_string(&mut suffix).expect("read EROFS suffix");
    assert_eq!(suffix, "EROFS!");
}

#[test]
fn rejects_compressed_inode_without_falling_back_to_plain_data() {
    let mut image = minimal_erofs_image();
    let inode = 2 * 4096 + 2 * 32;
    image[inode..inode + 2].copy_from_slice(&2u16.to_le_bytes());
    let reader =
        ErofsReader::open(Box::new(MemoryReader::new(image)), 0).expect("open compressed metadata");
    let error = reader
        .open_file("hello.txt")
        .err()
        .expect("compressed inode must be unsupported");
    assert_eq!(error.kind(), io::ErrorKind::Unsupported);
}

#[test]
fn reads_flat_plain_and_inline_symbolic_link_targets() {
    let inode = 2 * 4096 + 2 * 32;
    let mut plain = minimal_erofs_image();
    plain[inode + 4..inode + 6].copy_from_slice(&0xa1ffu16.to_le_bytes());
    let reader = ErofsReader::open(Box::new(MemoryReader::new(plain)), 0)
        .expect("open flat-plain symlink fixture");
    assert_eq!(
        reader
            .read_file_range("hello.txt", 0, 12)
            .expect("read flat-plain symlink target"),
        b"Hello EROFS!"
    );

    let mut inline = minimal_erofs_image();
    inline[inode..inode + 2].copy_from_slice(&4u16.to_le_bytes());
    inline[inode + 4..inode + 6].copy_from_slice(&0xa1ffu16.to_le_bytes());
    inline[inode + 8..inode + 12].copy_from_slice(&12u32.to_le_bytes());
    inline[inode + 32..inode + 44].copy_from_slice(b"Hello EROFS!");
    let reader = ErofsReader::open(Box::new(MemoryReader::new(inline)), 0)
        .expect("open flat-inline symlink fixture");
    assert_eq!(
        reader
            .read_file_range("hello.txt", 0, 12)
            .expect("read flat-inline symlink target"),
        b"Hello EROFS!"
    );
}

#[test]
fn reads_extended_flat_inode_and_validates_superblock_checksum() {
    let mut image = minimal_erofs_image();
    let inode = 2 * 4096 + 2 * 32;
    image[inode..inode + 2].copy_from_slice(&1u16.to_le_bytes());
    image[1024 + 8..1024 + 12].copy_from_slice(&1u32.to_le_bytes());
    let checksum = crc32c(0x5045_b54a, &image[1024 + 8..1024 + 3072]);
    image[1024 + 4..1024 + 8].copy_from_slice(&checksum.to_le_bytes());

    let reader = ErofsReader::open(Box::new(MemoryReader::new(image.clone())), 0)
        .expect("open extended checksummed EROFS");
    assert_eq!(
        reader
            .read_file_range("hello.txt", 0, 12)
            .expect("read extended flat file"),
        b"Hello EROFS!"
    );

    let mut corrupt = image;
    corrupt[1024 + 64] ^= 0xff;
    let error = ErofsReader::open(Box::new(MemoryReader::new(corrupt)), 0)
        .err()
        .expect("reject invalid superblock checksum");
    assert!(matches!(error, ErofsError::Invalid(_)));
}

#[test]
fn reads_flat_inline_tail_across_external_and_metadata_storage() {
    let mut image = minimal_erofs_image();
    let inode = 2 * 4096 + 2 * 32;
    image[inode..inode + 2].copy_from_slice(&4u16.to_le_bytes());
    image[inode + 2..inode + 4].copy_from_slice(&1u16.to_le_bytes());
    image[inode + 8..inode + 12].copy_from_slice(&4102u32.to_le_bytes());
    image[4 * 4096..5 * 4096].fill(b'A');
    image[inode + 44..inode + 50].copy_from_slice(b"TAIL!!");

    let reader =
        ErofsReader::open(Box::new(MemoryReader::new(image)), 0).expect("open flat-inline EROFS");
    assert_eq!(
        reader
            .read_file_range("hello.txt", 4094, 8)
            .expect("read across external and inline tail"),
        b"AATAIL!!"
    );
}

#[test]
fn lists_flat_inline_directory_without_scanning_data_blocks() {
    let mut image = minimal_erofs_image();
    let metadata = 2 * 4096;
    let root_inode = metadata + 32;
    let old_file_inode = metadata + 64;
    let new_file_inode = metadata + 4 * 32;
    image.copy_within(old_file_inode..old_file_inode + 32, new_file_inode);
    image[3 * 4096 + 24..3 * 4096 + 32].copy_from_slice(&4u64.to_le_bytes());
    let directory = image[3 * 4096..3 * 4096 + 48].to_vec();
    image[root_inode..root_inode + 2].copy_from_slice(&4u16.to_le_bytes());
    image[root_inode + 8..root_inode + 12].copy_from_slice(&48u32.to_le_bytes());
    image[root_inode + 32..root_inode + 80].copy_from_slice(&directory);

    let reader = ErofsReader::open(Box::new(MemoryReader::new(image)), 0)
        .expect("open inline-directory EROFS");
    let children = reader.list_children("").expect("list inline directory");
    assert_eq!(children.len(), 1);
    assert_eq!(children[0].name, "hello.txt");
}

#[test]
fn rejects_truncated_source_and_bad_metadata() {
    let mut truncated = minimal_erofs_image();
    truncated.truncate(4096 * 15);
    let error = ErofsReader::open(Box::new(MemoryReader::new(truncated)), 0)
        .err()
        .expect("reject truncated EROFS source");
    assert!(matches!(error, ErofsError::Invalid(_)));

    let mut invalid = minimal_erofs_image();
    invalid[1024..1028].copy_from_slice(&0u32.to_le_bytes());
    let error = ErofsReader::open(Box::new(MemoryReader::new(invalid)), 0)
        .err()
        .expect("reject bad EROFS magic");
    assert!(matches!(error, ErofsError::Invalid(_)));
}

fn crc32c(mut crc: u32, bytes: &[u8]) -> u32 {
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0x82f6_3b78 & 0u32.wrapping_sub(crc & 1));
        }
    }
    crc
}
