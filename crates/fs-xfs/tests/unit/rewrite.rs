use super::*;

use crate::log::replay::{metadata_crc_is_valid, stamp_metadata_crc};
use crate::reader::{sb_off, S_IFDIR, XFS_INODE_MAGIC, XFS_SUPER_MAGIC};
use crate::{FORMAT_LOCAL, INODE_CORE_SIZE, INODE_CORE_SIZE_V3};
use evidence_core::{EvidenceReader, FileSystemReader, ReaderInfo};
use std::io::{self, Read, Seek, SeekFrom};

const BLOCK_SIZE: usize = 4096;
const INODE_SIZE: usize = 256;
const INODE_BASE: usize = 2 * BLOCK_SIZE;
const TARGET_INODE_OFFSET: usize = INODE_BASE + 2 * INODE_SIZE;
const DATA_OFFSET: usize = 4 * BLOCK_SIZE;
const LOG_OFFSET: usize = 8 * BLOCK_SIZE;
const FS_UUID: [u8; 16] = [0x5a; 16];
const S_IFREG: u16 = 0x8000;

struct MemoryReader {
    data: Vec<u8>,
    position: u64,
    info: ReaderInfo,
}

impl MemoryReader {
    fn new(data: Vec<u8>) -> Self {
        let size = data.len() as u64;
        Self {
            data,
            position: 0,
            info: ReaderInfo {
                path: "rewrite-xfs".into(),
                size,
                kind: "rewrite-xfs".to_string(),
            },
        }
    }
}

impl Read for MemoryReader {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let start = self.position as usize;
        let end = start.saturating_add(buffer.len()).min(self.data.len());
        let read = end.saturating_sub(start);
        buffer[..read].copy_from_slice(&self.data[start..end]);
        self.position += read as u64;
        Ok(read)
    }
}

impl Seek for MemoryReader {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        self.position = match position {
            SeekFrom::Start(value) => value,
            SeekFrom::End(value) => (self.data.len() as i64 + value).max(0) as u64,
            SeekFrom::Current(value) => (self.position as i64 + value).max(0) as u64,
        };
        Ok(self.position)
    }
}

impl EvidenceReader for MemoryReader {
    fn info(&self) -> &ReaderInfo {
        &self.info
    }
}

fn encode_extent(logical: u64, start_block: u64, block_count: u64, unwritten: bool) -> [u8; 16] {
    let state = u64::from(unwritten) << 63;
    let l0 = state | ((logical & ((1u64 << 54) - 1)) << 9) | (start_block >> 43);
    let l1 = ((start_block & ((1u64 << 43) - 1)) << 21) | block_count;
    let mut encoded = [0; 16];
    encoded[..8].copy_from_slice(&l0.to_be_bytes());
    encoded[8..].copy_from_slice(&l1.to_be_bytes());
    encoded
}

fn rewrite_fixture() -> Vec<u8> {
    let mut image = vec![0u8; 10 * BLOCK_SIZE];
    let superblock = &mut image[..512];
    superblock[sb_off::MAGIC..sb_off::MAGIC + 4].copy_from_slice(&XFS_SUPER_MAGIC.to_be_bytes());
    superblock[sb_off::BLOCKSIZE..sb_off::BLOCKSIZE + 4]
        .copy_from_slice(&(BLOCK_SIZE as u32).to_be_bytes());
    superblock[sb_off::DBLOCKS..sb_off::DBLOCKS + 8].copy_from_slice(&10u64.to_be_bytes());
    superblock[sb_off::UUID..sb_off::UUID + 16].copy_from_slice(&FS_UUID);
    superblock[sb_off::LOGSTART..sb_off::LOGSTART + 8].copy_from_slice(&8u64.to_be_bytes());
    superblock[sb_off::ROOTINO..sb_off::ROOTINO + 8].copy_from_slice(&2u64.to_be_bytes());
    superblock[sb_off::AGBLOCKS..sb_off::AGBLOCKS + 4].copy_from_slice(&10u32.to_be_bytes());
    superblock[sb_off::AGCOUNT..sb_off::AGCOUNT + 4].copy_from_slice(&1u32.to_be_bytes());
    superblock[sb_off::LOGBLOCKS..sb_off::LOGBLOCKS + 4].copy_from_slice(&1u32.to_be_bytes());
    superblock[sb_off::VERSIONNUM..sb_off::VERSIONNUM + 2].copy_from_slice(&5u16.to_be_bytes());
    superblock[sb_off::SECTSIZE..sb_off::SECTSIZE + 2].copy_from_slice(&512u16.to_be_bytes());
    superblock[sb_off::INODESIZE..sb_off::INODESIZE + 2]
        .copy_from_slice(&(INODE_SIZE as u16).to_be_bytes());
    superblock[sb_off::INOPBLOCK..sb_off::INOPBLOCK + 2].copy_from_slice(&16u16.to_be_bytes());

    let root = &mut image[INODE_BASE + INODE_SIZE..INODE_BASE + 2 * INODE_SIZE];
    root[di_off::MAGIC..di_off::MAGIC + 2].copy_from_slice(&XFS_INODE_MAGIC.to_be_bytes());
    root[di_off::MODE..di_off::MODE + 2].copy_from_slice(&(S_IFDIR | 0o755).to_be_bytes());
    root[di_off::VERSION] = 2;
    root[di_off::FORMAT] = FORMAT_LOCAL;
    let fork = &mut root[INODE_CORE_SIZE..];
    fork[0] = 1;
    fork[1] = 1;
    fork[2..10].copy_from_slice(&2u64.to_be_bytes());
    fork[10] = 8;
    fork[11..13].copy_from_slice(&0x18u16.to_be_bytes());
    fork[13..21].copy_from_slice(b"test.txt");
    fork[21..29].copy_from_slice(&3u64.to_be_bytes());

    let inode = target_inode(&mut image);
    inode[di_off::MAGIC..di_off::MAGIC + 2].copy_from_slice(&XFS_INODE_MAGIC.to_be_bytes());
    inode[di_off::MODE..di_off::MODE + 2].copy_from_slice(&(S_IFREG | 0o600).to_be_bytes());
    inode[di_off::VERSION] = 3;
    inode[di_off::FORMAT] = FORMAT_EXTENTS;
    inode[di_off::SIZE..di_off::SIZE + 8].copy_from_slice(&11u64.to_be_bytes());
    inode[di_off::NEXTENTS..di_off::NEXTENTS + 4].copy_from_slice(&1u32.to_be_bytes());
    inode[112..120].copy_from_slice(&0x1234_5678u64.to_be_bytes());
    inode[152..160].copy_from_slice(&3u64.to_be_bytes());
    inode[160..176].copy_from_slice(&FS_UUID);
    inode[INODE_CORE_SIZE_V3..INODE_CORE_SIZE_V3 + 16]
        .copy_from_slice(&encode_extent(0, 4, 1, false));
    stamp_metadata_crc(inode);
    image[DATA_OFFSET..DATA_OFFSET + 11].copy_from_slice(b"Hello World");
    image
}

fn target_inode(image: &mut [u8]) -> &mut [u8] {
    &mut image[TARGET_INODE_OFFSET..TARGET_INODE_OFFSET + INODE_SIZE]
}

fn open(image: Vec<u8>) -> XfsReader {
    XfsReader::open(Box::new(MemoryReader::new(image)), 0).unwrap()
}

fn apply_plan(image: &mut [u8], plan: &XfsFileRewritePlan) {
    for patch in &plan.patches {
        let start = patch.volume_offset as usize;
        image[start..start + patch.bytes.len()].copy_from_slice(&patch.bytes);
    }
}

#[test]
fn plans_rewrite_tail_zeroing_inode_size_and_v3_crc() {
    let mut image = rewrite_fixture();
    let plan = open(image.clone())
        .plan_in_place_file_rewrite("test.txt", b"Hi")
        .unwrap();
    assert_eq!(plan.old_size, 11);
    let inode_patch = plan
        .patches
        .iter()
        .find(|patch| patch.volume_offset == TARGET_INODE_OFFSET as u64)
        .unwrap();
    assert_eq!(be_u64(&inode_patch.bytes, di_off::SIZE), 2);
    assert_eq!(&inode_patch.bytes[112..120], &0x1234_5678u64.to_be_bytes());
    assert!(metadata_crc_is_valid(&inode_patch.bytes));

    apply_plan(&mut image, &plan);
    assert_eq!(&image[DATA_OFFSET..DATA_OFFSET + 2], b"Hi");
    assert_eq!(&image[DATA_OFFSET + 2..DATA_OFFSET + 11], &[0; 9]);
    let filesystem = open(image);
    assert_eq!(filesystem.file_size_by_path("test.txt").unwrap(), 2);
    assert_eq!(filesystem.read_file_range("test.txt", 0, 2).unwrap(), b"Hi");
}

#[test]
fn rejects_dirty_log_growth_beyond_allocation_and_invalid_inode_crc() {
    let mut dirty = rewrite_fixture();
    dirty[LOG_OFFSET..LOG_OFFSET + 4].copy_from_slice(&1u32.to_be_bytes());
    assert!(open(dirty)
        .plan_in_place_file_rewrite("test.txt", b"Hi")
        .unwrap_err()
        .to_string()
        .contains("dirty"));

    let mut grown = rewrite_fixture();
    let growth = open(grown.clone())
        .plan_in_place_file_rewrite("test.txt", &[0x5a; 12])
        .expect("growth inside the existing written block is safe");
    assert_eq!(growth.old_size, 11);
    assert_eq!(
        be_u64(&growth.patches.last().unwrap().bytes, di_off::SIZE),
        12
    );
    apply_plan(&mut grown, &growth);
    let grown_fs = open(grown);
    assert_eq!(grown_fs.file_size_by_path("test.txt").unwrap(), 12);
    assert_eq!(
        grown_fs.read_file_range("test.txt", 0, 12).unwrap(),
        [0x5a; 12]
    );
    assert_eq!(
        open(rewrite_fixture())
            .plan_in_place_file_rewrite("test.txt", &[0; BLOCK_SIZE + 1])
            .unwrap_err()
            .kind(),
        io::ErrorKind::Unsupported
    );

    let mut invalid_crc = rewrite_fixture();
    target_inode(&mut invalid_crc)[100] ^= 1;
    assert!(open(invalid_crc)
        .plan_in_place_file_rewrite("test.txt", b"Hi")
        .unwrap_err()
        .to_string()
        .contains("CRC"));
}

#[test]
fn rejects_sparse_unwritten_and_reflink_layouts() {
    for (logical, unwritten, expected) in [(1, false, "gap"), (0, true, "unwritten")] {
        let mut image = rewrite_fixture();
        let inode = target_inode(&mut image);
        inode[INODE_CORE_SIZE_V3..INODE_CORE_SIZE_V3 + 16]
            .copy_from_slice(&encode_extent(logical, 4, 1, unwritten));
        stamp_metadata_crc(inode);
        assert!(open(image)
            .plan_in_place_file_rewrite("test.txt", b"Hi")
            .unwrap_err()
            .to_string()
            .contains(expected));
    }

    let mut reflink = rewrite_fixture();
    reflink[sb_off::FEATURES_RO_COMPAT..sb_off::FEATURES_RO_COMPAT + 4]
        .copy_from_slice(&(1u32 << 2).to_be_bytes());
    assert!(open(reflink)
        .plan_in_place_file_rewrite("test.txt", b"Hi")
        .unwrap_err()
        .to_string()
        .contains("reflink"));
}

#[test]
fn rejects_two_logical_extents_that_alias_one_physical_block() {
    let mut image = rewrite_fixture();
    let inode = target_inode(&mut image);
    inode[di_off::SIZE..di_off::SIZE + 8].copy_from_slice(&(2 * BLOCK_SIZE as u64).to_be_bytes());
    inode[di_off::NEXTENTS..di_off::NEXTENTS + 4].copy_from_slice(&2u32.to_be_bytes());
    inode[INODE_CORE_SIZE_V3..INODE_CORE_SIZE_V3 + 16]
        .copy_from_slice(&encode_extent(0, 4, 1, false));
    inode[INODE_CORE_SIZE_V3 + 16..INODE_CORE_SIZE_V3 + 32]
        .copy_from_slice(&encode_extent(1, 4, 1, false));
    stamp_metadata_crc(inode);

    assert!(open(image)
        .plan_in_place_file_rewrite("test.txt", b"Hi")
        .unwrap_err()
        .to_string()
        .contains("physical overlap"));
}
