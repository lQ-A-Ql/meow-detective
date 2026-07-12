use super::{
    DIR2_SF_HDR_8, XFS_DIR2_BLOCK_MAGIC, XFS_DIR2_DATA_ALIGN, XFS_DIR2_DATA_HDR_SIZE,
    XFS_DIR2_DATA_MAGIC, XFS_DIR2_FREE_TAG, XFS_DIR3_BLOCK_MAGIC, XFS_DIR3_DATA_HDR_SIZE,
    XFS_DIR3_DATA_MAGIC, XFS_DIR3_FT_DIR,
};
use crate::reader::{
    sb_off, INODE_CORE_SIZE, S_IFDIR, XFS_INODE_MAGIC, XFS_SB_FEAT_INCOMPAT_FTYPE, XFS_SUPER_MAGIC,
};
use crate::{
    be_u16, di_off, XfsReader, BMAP_MAGIC, BMBT_SHORT_ROOT_HDR_SIZE, FORMAT_BTREE, FORMAT_EXTENTS,
    FORMAT_LOCAL,
};
use evidence_core::filesystem::FileSystemReader;
use evidence_core::EvidenceReader;
use evidence_core::ReaderInfo;
use std::io::{self, SeekFrom};
use std::io::{Read, Seek};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

const S_IFREG: u16 = 0x8000;
const XFS_DIR3_FT_REG_FILE: u8 = 1;

fn parse_block_dir(data: &[u8]) -> io::Result<Vec<(String, u64, bool)>> {
    Ok(parse_block_dir_entries(data)?
        .into_iter()
        .map(|entry| {
            (
                entry.name,
                entry.inode,
                entry.ftype == Some(XFS_DIR3_FT_DIR),
            )
        })
        .collect())
}

fn parse_block_dir_entries(data: &[u8]) -> io::Result<Vec<crate::directory::XfsDirectoryEntry>> {
    let with_ftype = XfsReader::parse_block_dir_entries_impl(data, false, true);
    let parsed = if !with_ftype.entries.is_empty() || with_ftype.error.is_some() {
        with_ftype
    } else {
        XfsReader::parse_block_dir_entries_impl(data, false, false)
    };
    parsed.error.map_or_else(|| Ok(parsed.entries), Err)
}
// -----------------------------------------------------------------------
// Fake evidence reader for in-memory fixtures
// -----------------------------------------------------------------------

struct FakeReader {
    data: Vec<u8>,
    pos: u64,
    info: ReaderInfo,
}

impl FakeReader {
    fn new(data: Vec<u8>) -> Self {
        Self {
            data,
            pos: 0,
            info: ReaderInfo {
                path: std::path::PathBuf::from("fake-xfs"),
                size: 0,
                kind: "fake-xfs".to_string(),
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

impl EvidenceReader for FakeReader {
    fn info(&self) -> &ReaderInfo {
        &self.info
    }
}

struct CountingReader {
    inner: FakeReader,
    bytes_read: Arc<AtomicUsize>,
}

impl CountingReader {
    fn new(data: Vec<u8>, bytes_read: Arc<AtomicUsize>) -> Self {
        Self {
            inner: FakeReader::new(data),
            bytes_read,
        }
    }
}

impl Read for CountingReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.bytes_read.fetch_add(n, Ordering::Relaxed);
        Ok(n)
    }
}

impl Seek for CountingReader {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        self.inner.seek(pos)
    }
}

impl EvidenceReader for CountingReader {
    fn info(&self) -> &ReaderInfo {
        self.inner.info()
    }
}

struct PartialRangeReader {
    data: Vec<u8>,
    pos: u64,
    partial_start: usize,
    partial_end: usize,
    info: ReaderInfo,
}

impl PartialRangeReader {
    fn new(data: Vec<u8>, partial_start: usize, partial_end: usize) -> Self {
        let size = data.len() as u64;
        Self {
            data,
            pos: 0,
            partial_start,
            partial_end,
            info: ReaderInfo {
                size,
                path: std::path::PathBuf::from("partial-range-xfs"),
                kind: "partial-range-xfs".to_string(),
            },
        }
    }
}

impl Read for PartialRangeReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }

        let start = self.pos as usize;
        if start >= self.data.len() || (start >= self.partial_start && start < self.partial_end) {
            return Ok(0);
        }

        let mut end = (start + buf.len()).min(self.data.len());
        if start < self.partial_start {
            end = end.min(self.partial_start);
        }
        let n = end.saturating_sub(start);
        buf[..n].copy_from_slice(&self.data[start..end]);
        self.pos += n as u64;
        Ok(n)
    }
}

impl Seek for PartialRangeReader {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        self.pos = match pos {
            SeekFrom::Start(p) => p,
            SeekFrom::End(p) => (self.data.len() as i64 + p).max(0) as u64,
            SeekFrom::Current(p) => (self.pos as i64 + p).max(0) as u64,
        };
        Ok(self.pos)
    }
}

impl EvidenceReader for PartialRangeReader {
    fn info(&self) -> &ReaderInfo {
        &self.info
    }
}

fn encode_bmbt_extent(logical: u64, start_block: u64, block_count: u64) -> [u8; 16] {
    let l0 = ((logical & ((1u64 << 54) - 1)) << 9) | (start_block >> 43);
    let l1 = ((start_block & ((1u64 << 43) - 1)) << 21) | (block_count & 0x1F_FFFF);
    let mut encoded = [0u8; 16];
    encoded[0..8].copy_from_slice(&l0.to_be_bytes());
    encoded[8..16].copy_from_slice(&l1.to_be_bytes());
    encoded
}

// -----------------------------------------------------------------------
// Fixture builder
// -----------------------------------------------------------------------
//
// Layout (block_size = 4096, 10 blocks total, agcount = 1):
//
// | Block | Offset   | Content                              |
// |-------|----------|--------------------------------------|
// | 0     | 0        | Superblock                           |
// | 1     | 4096     | (unused gap)                         |
// | 2     | 8192     | Inode table (5 × 256 bytes)          |
// | 3     | 12288    | (unused)                             |
// | 4     | 16384    | "test.txt" file data                 |
// | 5     | 20480    | (unused)                             |
// | 6     | 24576    | "hello.dat" file data                |
//
// Inodes:
//   ino 1 — reserved / zeroed
//   ino 2 — root dir   (format=1 LOCAL shortform)
//   ino 3 — test.txt   (format=2 EXTENTS)
//   ino 4 — subdir     (format=1 LOCAL shortform)
//   ino 5 — hello.dat  (format=3 BTREE, leaf-level bmbt)

fn build_xfs_fixture() -> Vec<u8> {
    let block_size: u64 = 4096;
    let total_blocks: u64 = 10;
    let total_size = (total_blocks * block_size) as usize;
    let mut img = vec![0u8; total_size];

    // ---- Superblock at offset 0 ----
    let sb = &mut img[0..512];
    sb[0x00..0x04].copy_from_slice(&XFS_SUPER_MAGIC.to_be_bytes()); // sb_magicnum
    sb[0x04..0x08].copy_from_slice(&(block_size as u32).to_be_bytes()); // sb_blocksize
    sb[0x08..0x10].copy_from_slice(&total_blocks.to_be_bytes()); // sb_dblocks
    sb[0x38..0x40].copy_from_slice(&2u64.to_be_bytes()); // sb_rootino = 2
    sb[0x54..0x58].copy_from_slice(&(total_blocks as u32).to_be_bytes()); // sb_agblocks = 10
    sb[0x58..0x5C].copy_from_slice(&1u32.to_be_bytes()); // sb_agcount = 1
    sb[0x66..0x68].copy_from_slice(&512u16.to_be_bytes()); // sb_sectsize
    sb[0x68..0x6A].copy_from_slice(&256u16.to_be_bytes()); // sb_inodesize = 256
    sb[0x6A..0x6C].copy_from_slice(&16u16.to_be_bytes()); // sb_inopblock = 16
    sb[sb_off::DIRBLKLOG] = 0;

    // ---- Inode table at block 2 (offset 8192) ----
    let ino_base: usize = 8192;
    let ino_size: usize = 256;

    // -- Inode 1: reserved (zeroed) --
    // (already zero)

    // -- Inode 2: root directory (LOCAL / shortform) --
    let ri = &mut img[ino_base + ino_size..ino_base + 2 * ino_size];
    ri[di_off::MAGIC..di_off::MAGIC + 2].copy_from_slice(&XFS_INODE_MAGIC.to_be_bytes());
    ri[di_off::MODE..di_off::MODE + 2].copy_from_slice(&(S_IFDIR | 0o755).to_be_bytes());
    ri[di_off::FORMAT] = FORMAT_LOCAL;
    ri[di_off::SIZE..di_off::SIZE + 8].copy_from_slice(&4096u64.to_be_bytes());
    ri[di_off::FORKOFF] = 0;

    // Data fork: shortform dir with 2 entries.
    let df_root = &mut ri[INODE_CORE_SIZE..];
    df_root[0] = 2; // count
    df_root[1] = 2; // i8count=count → all entries 8-byte inodes
    df_root[2..10].copy_from_slice(&2u64.to_be_bytes()); // parent = ino 2
    let mut pos = DIR2_SF_HDR_8;

    // Entry: "test.txt" → ino 3
    df_root[pos] = 8; // namelen
    pos += 1;
    // offset (2 bytes, arbitrary)
    df_root[pos..pos + 2].copy_from_slice(&0x0018u16.to_be_bytes());
    pos += 2;
    df_root[pos..pos + 8].copy_from_slice(b"test.txt");
    pos += 8;
    df_root[pos..pos + 8].copy_from_slice(&3u64.to_be_bytes()); // inode 3
    pos += 8;

    // Entry: "subdir" → ino 4
    df_root[pos] = 6; // namelen
    pos += 1;
    df_root[pos..pos + 2].copy_from_slice(&0x0040u16.to_be_bytes());
    pos += 2;
    df_root[pos..pos + 6].copy_from_slice(b"subdir");
    pos += 6;
    df_root[pos..pos + 8].copy_from_slice(&4u64.to_be_bytes()); // inode 4

    // -- Inode 3: test.txt (EXTENTS) --
    let fi = &mut img[ino_base + 2 * ino_size..ino_base + 3 * ino_size];
    fi[di_off::MAGIC..di_off::MAGIC + 2].copy_from_slice(&XFS_INODE_MAGIC.to_be_bytes());
    fi[di_off::MODE..di_off::MODE + 2].copy_from_slice(&(S_IFREG | 0o644).to_be_bytes());
    fi[di_off::FORMAT] = FORMAT_EXTENTS;
    fi[di_off::SIZE..di_off::SIZE + 8].copy_from_slice(&11u64.to_be_bytes()); // "Hello World"
    fi[di_off::NEXTENTS..di_off::NEXTENTS + 4].copy_from_slice(&1u32.to_be_bytes());
    fi[di_off::FORKOFF] = 0;

    // One extent record: logical=0, start=block 4, count=1.
    let df_file = &mut fi[INODE_CORE_SIZE..];
    df_file[0..16].copy_from_slice(&encode_bmbt_extent(0, 4, 1));

    // -- Inode 4: subdir (LOCAL / shortform) --
    let sd = &mut img[ino_base + 3 * ino_size..ino_base + 4 * ino_size];
    sd[di_off::MAGIC..di_off::MAGIC + 2].copy_from_slice(&XFS_INODE_MAGIC.to_be_bytes());
    sd[di_off::MODE..di_off::MODE + 2].copy_from_slice(&(S_IFDIR | 0o755).to_be_bytes());
    sd[di_off::FORMAT] = FORMAT_LOCAL;
    sd[di_off::SIZE..di_off::SIZE + 8].copy_from_slice(&4096u64.to_be_bytes());
    sd[di_off::FORKOFF] = 0;

    let df_sd = &mut sd[INODE_CORE_SIZE..];
    df_sd[0] = 1; // count
    df_sd[1] = 1; // i8count
    df_sd[2..10].copy_from_slice(&2u64.to_be_bytes()); // parent = ino 2
    let mut sd_pos = DIR2_SF_HDR_8;

    // Entry: "hello.dat" → ino 5
    df_sd[sd_pos] = 9; // namelen
    sd_pos += 1;
    df_sd[sd_pos..sd_pos + 2].copy_from_slice(&0x0018u16.to_be_bytes());
    sd_pos += 2;
    df_sd[sd_pos..sd_pos + 9].copy_from_slice(b"hello.dat");
    sd_pos += 9;
    df_sd[sd_pos..sd_pos + 8].copy_from_slice(&5u64.to_be_bytes()); // inode 5

    // -- Inode 5: hello.dat (BTREE, level-0 bmbt leaf) --
    let hi = &mut img[ino_base + 4 * ino_size..ino_base + 5 * ino_size];
    hi[di_off::MAGIC..di_off::MAGIC + 2].copy_from_slice(&XFS_INODE_MAGIC.to_be_bytes());
    hi[di_off::MODE..di_off::MODE + 2].copy_from_slice(&(S_IFREG | 0o644).to_be_bytes());
    hi[di_off::FORMAT] = FORMAT_BTREE;
    hi[di_off::SIZE..di_off::SIZE + 8].copy_from_slice(&13u64.to_be_bytes()); // "Hello subdir!"
    hi[di_off::FORKOFF] = 0;

    // Bmbt root block header (in data fork).
    let df_hi = &mut hi[INODE_CORE_SIZE..];
    df_hi[0..4].copy_from_slice(&BMAP_MAGIC.to_be_bytes()); // bb_magic
    df_hi[4..6].copy_from_slice(&0u16.to_be_bytes()); // bb_level = 0 (leaf)
    df_hi[6..8].copy_from_slice(&1u16.to_be_bytes()); // bb_numrecs = 1
                                                      // bmdr header is 8 bytes; leaf records follow immediately.
                                                      // Leaf record: key(8) + extent l0(8) + extent l1(8) = 24 bytes.
    let rec_off: usize = 8;
    df_hi[rec_off..rec_off + 8].copy_from_slice(&0u64.to_be_bytes()); // key = file block 0
    df_hi[rec_off + 8..rec_off + 24].copy_from_slice(&encode_bmbt_extent(0, 6, 1));

    // ---- Block 4: test.txt data "Hello World" ----
    img[16384..16384 + 11].copy_from_slice(b"Hello World");

    // ---- Block 6: hello.dat data "Hello subdir!" ----
    img[24576..24576 + 13].copy_from_slice(b"Hello subdir!");

    img
}

fn build_large_sparse_xfs_fixture(marker: &[u8]) -> (Vec<u8>, u64) {
    const LOGICAL_OFFSET: u64 = 128 * 1024 * 1024;
    let mut img = build_xfs_fixture();
    let block_size = 4096u64;
    let logical_block = LOGICAL_OFFSET / block_size;
    let physical_block = 4u64;
    let file_size = LOGICAL_OFFSET + marker.len() as u64;

    let fi = &mut img[8192 + 2 * 256..8192 + 3 * 256];
    fi[di_off::SIZE..di_off::SIZE + 8].copy_from_slice(&file_size.to_be_bytes());
    let df_file = &mut fi[INODE_CORE_SIZE..];
    df_file[0..16].copy_from_slice(&encode_bmbt_extent(logical_block, physical_block, 1));

    let data_offset = physical_block as usize * block_size as usize;
    img[data_offset..data_offset + marker.len()].copy_from_slice(marker);
    (img, LOGICAL_OFFSET)
}

fn build_small_sparse_xfs_fixture(marker: &[u8]) -> (Vec<u8>, u64) {
    let mut img = build_xfs_fixture();
    let block_size = 4096u64;
    let logical_offset = 2 * block_size;
    let logical_block = logical_offset / block_size;
    let physical_block = 4u64;
    let file_size = logical_offset + marker.len() as u64;

    let fi = &mut img[8192 + 2 * 256..8192 + 3 * 256];
    fi[di_off::SIZE..di_off::SIZE + 8].copy_from_slice(&file_size.to_be_bytes());
    let df_file = &mut fi[INODE_CORE_SIZE..];
    df_file[0..16].copy_from_slice(&encode_bmbt_extent(logical_block, physical_block, 1));

    let data_offset = physical_block as usize * block_size as usize;
    img[data_offset..data_offset + marker.len()].copy_from_slice(marker);
    (img, logical_offset)
}

fn build_truncated_extent_xfs_fixture(marker: &[u8]) -> Vec<u8> {
    let mut img = build_xfs_fixture();
    let block_size = 4096usize;

    let fi = &mut img[8192 + 2 * 256..8192 + 3 * 256];
    fi[di_off::SIZE..di_off::SIZE + 8].copy_from_slice(&(block_size as u64).to_be_bytes());
    let df_file = &mut fi[INODE_CORE_SIZE..];
    df_file[0..16].copy_from_slice(&encode_bmbt_extent(0, 4, 1));

    let data_offset = 4 * block_size;
    img.truncate(data_offset + marker.len());
    img[data_offset..data_offset + marker.len()].copy_from_slice(marker);
    img
}

fn build_xfs_fixture_with_btree_child_magic(child_magic: u32) -> Vec<u8> {
    let mut img = build_xfs_fixture();
    let block_size = 4096usize;

    let fi = &mut img[8192 + 2 * 256..8192 + 3 * 256];
    fi[di_off::FORMAT] = FORMAT_BTREE;
    fi[di_off::SIZE..di_off::SIZE + 8].copy_from_slice(&1u64.to_be_bytes());
    fi[di_off::NEXTENTS..di_off::NEXTENTS + 4].copy_from_slice(&1u32.to_be_bytes());
    fi[di_off::FORKOFF] = 0;

    let df = &mut fi[INODE_CORE_SIZE..];
    df.fill(0);
    df[0..2].copy_from_slice(&1u16.to_be_bytes());
    df[2..4].copy_from_slice(&1u16.to_be_bytes());
    df[4..12].copy_from_slice(&0u64.to_be_bytes());
    let maxrecs = (df.len() - BMBT_SHORT_ROOT_HDR_SIZE) / 16;
    let ptrs_start = BMBT_SHORT_ROOT_HDR_SIZE + maxrecs * 8;
    df[ptrs_start..ptrs_start + 8].copy_from_slice(&7u64.to_be_bytes());

    let block7 = 7 * block_size;
    img[block7..block7 + block_size].fill(0);
    img[block7..block7 + 4].copy_from_slice(&child_magic.to_be_bytes());
    img
}

fn build_xfs_fixture_with_zeroed_block_dir_and_residual_shortform() -> Vec<u8> {
    let mut img = build_xfs_fixture();
    let block_size = 4096u64;

    let fi = &mut img[8192 + 2 * 256..8192 + 3 * 256];
    fi[di_off::MODE..di_off::MODE + 2].copy_from_slice(&(S_IFDIR | 0o755).to_be_bytes());
    fi[di_off::FORMAT] = FORMAT_EXTENTS;
    fi[di_off::SIZE..di_off::SIZE + 8].copy_from_slice(&block_size.to_be_bytes());
    fi[di_off::NEXTENTS..di_off::NEXTENTS + 4].copy_from_slice(&1u32.to_be_bytes());
    fi[di_off::FORKOFF] = 2;

    let core = INODE_CORE_SIZE;
    let df_file = &mut fi[core..core + 16];
    df_file.copy_from_slice(&encode_bmbt_extent(0, 7, 1));

    let residual = &mut fi[core + 16..];
    residual[0] = 1;
    residual[1] = 1;
    residual[2..10].copy_from_slice(&2u64.to_be_bytes());
    let mut pos = DIR2_SF_HDR_8;
    residual[pos] = 6;
    pos += 1;
    residual[pos..pos + 2].copy_from_slice(&0x0018u16.to_be_bytes());
    pos += 2;
    residual[pos..pos + 6].copy_from_slice(b"subdir");
    pos += 6;
    residual[pos..pos + 8].copy_from_slice(&4u64.to_be_bytes());

    let block7 = 7usize * block_size as usize;
    img[block7..block7 + block_size as usize].fill(0);
    img
}

fn build_xfs_fixture_with_zeroed_block_dir_without_residual_shortform() -> Vec<u8> {
    let mut img = build_xfs_fixture_with_zeroed_block_dir_and_residual_shortform();
    let fi = &mut img[8192 + 2 * 256..8192 + 3 * 256];
    fi[INODE_CORE_SIZE + 16..].fill(0);
    img
}

// -----------------------------------------------------------------------
// test_superblock_magic
// -----------------------------------------------------------------------

#[test]
fn test_superblock_magic() {
    let img = build_xfs_fixture();
    let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
    let xfs = XfsReader::open(reader, 0).unwrap();
    assert_eq!(xfs.data_source_name(), "xfs");
    assert_eq!(xfs.block_size, 4096);
}

// -----------------------------------------------------------------------
// test_ag_count
// -----------------------------------------------------------------------

#[test]
fn test_ag_count() {
    let img = build_xfs_fixture();
    let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
    let xfs = XfsReader::open(reader, 0).unwrap();
    // Fixture declares agcount = 1, agblocks = 10 (== dblocks).
    assert_eq!(xfs._ag_count, 1);
    assert_eq!(xfs._ag_blocks, 10);
}

// -----------------------------------------------------------------------
// test_inode_magic
// -----------------------------------------------------------------------

#[test]
fn test_inode_magic() {
    let img = build_xfs_fixture();
    let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
    let xfs = XfsReader::open(reader, 0).unwrap();

    let inode = xfs.read_inode(2).unwrap();
    assert_eq!(be_u16(&inode, di_off::MAGIC), XFS_INODE_MAGIC);
    assert_eq!(inode[di_off::FORMAT], FORMAT_LOCAL);
    assert!(XfsReader::inode_is_dir(&inode));

    let file_inode = xfs.read_inode(3).unwrap();
    assert_eq!(be_u16(&file_inode, di_off::MAGIC), XFS_INODE_MAGIC);
    assert_eq!(file_inode[di_off::FORMAT], FORMAT_EXTENTS);
    assert!(!XfsReader::inode_is_dir(&file_inode));
}

// -----------------------------------------------------------------------
// test_root_directory
// -----------------------------------------------------------------------

#[test]
fn test_root_directory() {
    let img = build_xfs_fixture();
    let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
    let xfs = XfsReader::open(reader, 0).unwrap();

    let root = xfs.root().unwrap();
    assert_eq!(root.name, "\\");
    assert!(root.is_dir);
    assert_eq!(root.size, 0);

    let children = xfs.list_children("").unwrap();
    let names: Vec<&str> = children.iter().map(|n| n.name.as_str()).collect();
    assert!(names.contains(&"test.txt"));
    assert!(names.contains(&"subdir"));
    assert_eq!(children.len(), 2);

    let txt = children.iter().find(|n| n.name == "test.txt").unwrap();
    assert!(!txt.is_dir);
    assert_eq!(txt.path, "test.txt");

    let sub = children.iter().find(|n| n.name == "subdir").unwrap();
    assert!(sub.is_dir);
    assert_eq!(sub.path, "subdir");
}

// -----------------------------------------------------------------------
// test_file_read
// -----------------------------------------------------------------------

#[test]
fn test_file_read() {
    let img = build_xfs_fixture();
    let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
    let xfs = XfsReader::open(reader, 0).unwrap();

    // Read extent-mapped file.
    let mut file = xfs.open_file("test.txt").unwrap();
    let mut content = String::new();
    file.read_to_string(&mut content).unwrap();
    assert_eq!(content, "Hello World");

    // Read B+tree-mapped file in subdirectory.
    let mut file2 = xfs.open_file("subdir/hello.dat").unwrap();
    let mut content2 = String::new();
    file2.read_to_string(&mut content2).unwrap();
    assert_eq!(content2, "Hello subdir!");
}

#[test]
fn test_large_sparse_file_range_reads_only_requested_extent() {
    let marker = b"XFS-RANGE-ONLY";
    let (img, offset) = build_large_sparse_xfs_fixture(marker);
    let bytes_read = Arc::new(AtomicUsize::new(0));
    let reader: Box<dyn EvidenceReader> =
        Box::new(CountingReader::new(img, Arc::clone(&bytes_read)));
    let xfs = XfsReader::open(reader, 0).unwrap();

    bytes_read.store(0, Ordering::Relaxed);
    let bytes = xfs
        .read_file_range("test.txt", offset, marker.len())
        .unwrap();

    assert_eq!(bytes, marker);
    assert!(
        bytes_read.load(Ordering::Relaxed) < 32 * 1024,
        "range path should not read the 128 MiB sparse prefix"
    );
}

#[test]
fn test_bmbt_extent_decode_real_bit_layout() {
    let logical = (1u64 << 40) + 7;
    let start_block = (1u64 << 44) + 0x12345;
    let block_count = 0x1F;
    let encoded = encode_bmbt_extent(logical, start_block, block_count);

    let decoded = XfsReader::decode_extent(&encoded);

    assert_eq!(decoded.logical, logical);
    assert_eq!(decoded.start_block, start_block);
    assert_eq!(decoded.block_count, block_count);
    assert!(!decoded.unwritten);
}

#[test]
fn test_fsblock_to_linear_block_uses_ag_geometry() {
    let img = build_xfs_fixture();
    let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
    let mut xfs = XfsReader::open(reader, 0).unwrap();
    xfs.agblklog = 4;
    xfs._ag_blocks = 10;
    xfs._ag_count = 3;
    xfs.dblocks = 30;

    assert_eq!(xfs.fsblock_to_linear_block(0x12).unwrap(), 12);
    assert!(xfs.fsblock_to_linear_block(0x1A).is_err());
}

#[test]
fn test_add_fsblocks_within_ag_rejects_boundary_crossing() {
    let img = build_xfs_fixture();
    let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
    let mut xfs = XfsReader::open(reader, 0).unwrap();
    xfs.agblklog = 4;
    xfs._ag_blocks = 10;
    xfs._ag_count = 2;
    xfs.dblocks = 20;

    assert_eq!(xfs.add_fsblocks_within_ag(0x11, 8).unwrap(), 0x19);
    let err = xfs.add_fsblocks_within_ag(0x11, 9).unwrap_err();

    assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    assert!(err.to_string().contains("crosses XFS AG boundary"));
}

#[test]
fn test_directory_block_fsblocks_uses_superblock_dirblklog() {
    let mut img = build_xfs_fixture();
    img[sb_off::DIRBLKLOG] = 2;
    let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
    let xfs = XfsReader::open(reader, 0).unwrap();

    assert_eq!(xfs.directory_block_fsblocks().unwrap(), 4);
}

#[test]
fn test_data_fork_forkoff_is_64bit_word_units() {
    let mut inode = vec![0u8; 256];
    inode[di_off::FORMAT] = FORMAT_EXTENTS;
    inode[di_off::FORKOFF] = 2;
    inode[INODE_CORE_SIZE..INODE_CORE_SIZE + 16].copy_from_slice(&encode_bmbt_extent(0, 4, 1));

    let data_fork = XfsReader::data_fork(&inode).unwrap();

    assert_eq!(data_fork.len(), 16);
    assert_eq!(XfsReader::decode_extent(data_fork).start_block, 4);
}

#[test]
fn test_range_extent_read_errors_on_truncated_allocated_extent() {
    let marker = b"TRUNCATED";
    let (mut img, offset) = build_large_sparse_xfs_fixture(marker);
    img.truncate(4 * 4096 + marker.len() - 1);
    let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
    let xfs = XfsReader::open(reader, 0).unwrap();

    let err = xfs
        .read_file_range("test.txt", offset, marker.len())
        .unwrap_err();

    assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    assert!(err
        .to_string()
        .contains("allocated XFS extent read truncated"));
}

#[test]
fn test_full_extent_read_errors_on_truncated_allocated_extent() {
    let marker = b"PARTIAL";
    let img = build_truncated_extent_xfs_fixture(marker);
    let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
    let xfs = XfsReader::open(reader, 0).unwrap();

    let err = match xfs.open_file("test.txt") {
        Ok(_) => panic!("expected truncated allocated extent read to fail"),
        Err(err) => err,
    };

    assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    assert!(err
        .to_string()
        .contains("allocated XFS extent read truncated"));
}

#[test]
fn test_full_extent_read_preserves_sparse_logical_hole() {
    let marker = b"TAIL";
    let (img, logical_offset) = build_small_sparse_xfs_fixture(marker);
    let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
    let xfs = XfsReader::open(reader, 0).unwrap();

    let mut file = xfs.open_file("test.txt").unwrap();
    let mut data = Vec::new();
    file.read_to_end(&mut data).unwrap();

    assert_eq!(data.len(), logical_offset as usize + marker.len());
    assert!(data[..logical_offset as usize]
        .iter()
        .all(|byte| *byte == 0));
    assert_eq!(&data[logical_offset as usize..], marker);
}

#[test]
fn test_bmbt_child_zero_magic_returns_invalid_data() {
    let img = build_xfs_fixture_with_btree_child_magic(0);
    let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
    let xfs = XfsReader::open(reader, 0).unwrap();

    let err = match xfs.open_file("test.txt") {
        Ok(_) => panic!("expected bmbt child zero magic to fail"),
        Err(err) => err,
    };

    assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    let message = err.to_string();
    assert!(message.contains("magic 0x00000000"));
    assert!(message.contains("FSB 7"));
}

#[test]
fn test_bmbt_child_unknown_magic_returns_invalid_data() {
    let img = build_xfs_fixture_with_btree_child_magic(0x4241_4421);
    let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
    let xfs = XfsReader::open(reader, 0).unwrap();

    let err = match xfs.open_file("test.txt") {
        Ok(_) => panic!("expected bmbt child unknown magic to fail"),
        Err(err) => err,
    };

    assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    let message = err.to_string();
    assert!(message.contains("magic 0x42414421"));
    assert!(message.contains("FSB 7"));
}

#[test]
fn test_block_to_offset_overflow_returns_invalid_data() {
    let img = build_xfs_fixture();
    let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
    let mut xfs = XfsReader::open(reader, 0).unwrap();
    xfs.agblklog = 0;
    xfs.block_size = 4096;

    let err = xfs.fsblock_to_offset(u64::MAX).unwrap_err();

    assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    assert!(
        err.to_string().contains("overflows")
            || err.to_string().contains("outside XFS data blocks")
    );
}

#[test]
fn test_inode_offset_bad_geometry_returns_invalid_data() {
    let img = build_xfs_fixture();
    let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
    let mut xfs = XfsReader::open(reader, 0).unwrap();
    xfs.agblklog = 63;
    xfs.inopblog = 1;

    let err = xfs.read_inode(2).unwrap_err();

    assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    assert!(err.to_string().contains("inode geometry"));
}

// -----------------------------------------------------------------------
// test_invalid_magic_rejected
// -----------------------------------------------------------------------

#[test]
fn test_invalid_magic_rejected() {
    let mut img = build_xfs_fixture();
    // Corrupt the superblock magic.
    img[0] = 0x00;
    img[1] = 0x00;
    img[2] = 0x00;
    img[3] = 0x00;

    let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
    match XfsReader::open(reader, 0) {
        Ok(_) => panic!("expected error for invalid magic"),
        Err(err) => {
            assert_eq!(err.kind(), io::ErrorKind::InvalidData);
            assert!(err.to_string().contains("magic"));
        }
    }
}

// -----------------------------------------------------------------------
// test_inode_size_and_sectsize
// -----------------------------------------------------------------------

#[test]
fn test_inode_size_and_sectsize() {
    let img = build_xfs_fixture();
    let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
    let xfs = XfsReader::open(reader, 0).unwrap();
    assert_eq!(xfs.inode_size, 256);
    assert_eq!(xfs._inopblock, 16);
}

// -----------------------------------------------------------------------
// test_open_nonexistent_file
// -----------------------------------------------------------------------

#[test]
fn test_open_nonexistent_file() {
    let img = build_xfs_fixture();
    let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
    let xfs = XfsReader::open(reader, 0).unwrap();

    match xfs.open_file("nonexistent.txt") {
        Ok(_) => panic!("expected error for non-existent file"),
        Err(err) => assert_eq!(err.kind(), io::ErrorKind::NotFound),
    }
}

// -----------------------------------------------------------------------
// test_data_source_name
// -----------------------------------------------------------------------

#[test]
fn test_data_source_name() {
    let img = build_xfs_fixture();
    let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
    let xfs = XfsReader::open(reader, 0).unwrap();
    assert_eq!(xfs.data_source_name(), "xfs");
}

// -----------------------------------------------------------------------
// test_list_nonexistent_path
// -----------------------------------------------------------------------

#[test]
fn test_list_nonexistent_path() {
    let img = build_xfs_fixture();
    let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
    let xfs = XfsReader::open(reader, 0).unwrap();

    let err = xfs.list_children("no_such_dir").unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::NotFound);
}

// -----------------------------------------------------------------------
// test_btree_format_file_read
// -----------------------------------------------------------------------

#[test]
fn test_btree_format_file_read() {
    let img = build_xfs_fixture();
    let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
    let xfs = XfsReader::open(reader, 0).unwrap();

    // hello.dat is format=3 (BTREE).
    let mut file = xfs.open_file("subdir/hello.dat").unwrap();
    let mut content = String::new();
    file.read_to_string(&mut content).unwrap();
    assert_eq!(content, "Hello subdir!");
}

// -----------------------------------------------------------------------
// test_ag_geometry_computation
// -----------------------------------------------------------------------

#[test]
fn test_ag_geometry_computation() {
    // Build a fixture with sb_agblocks = 0 so XfsReader computes it.
    let block_size: u64 = 4096;
    let total_blocks: u64 = 20;
    let total_size = (total_blocks * block_size) as usize;
    let mut img = vec![0u8; total_size];

    let sb = &mut img[0..512];
    sb[0x00..0x04].copy_from_slice(&XFS_SUPER_MAGIC.to_be_bytes());
    sb[0x04..0x08].copy_from_slice(&(block_size as u32).to_be_bytes());
    sb[0x08..0x10].copy_from_slice(&total_blocks.to_be_bytes());
    sb[0x38..0x40].copy_from_slice(&2u64.to_be_bytes());
    // sb_agblocks = 0 (at 0x54), reader will compute dblocks / agcount.
    sb[0x58..0x5C].copy_from_slice(&4u32.to_be_bytes()); // agcount = 4
    sb[0x66..0x68].copy_from_slice(&512u16.to_be_bytes());
    sb[0x68..0x6A].copy_from_slice(&256u16.to_be_bytes());
    sb[0x6A..0x6C].copy_from_slice(&16u16.to_be_bytes());

    let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
    let xfs = XfsReader::open(reader, 0).unwrap();
    assert_eq!(xfs._ag_count, 4);
    // dblocks(20) / agcount(4) = 5
    assert_eq!(xfs._ag_blocks, 5);
}

// -----------------------------------------------------------------------
// test_inode_validation_rejects_bad_magic
// -----------------------------------------------------------------------

#[test]
fn test_inode_validation_rejects_bad_magic() {
    let mut img = build_xfs_fixture();
    // Corrupt inode 2's magic.
    let ino2_off: usize = 8192 + 256; // inode 2 offset
    img[ino2_off] = 0x00;
    img[ino2_off + 1] = 0x00;

    let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
    let xfs = XfsReader::open(reader, 0).unwrap();

    // read_file_content validates magic and should fail.
    let result = xfs.read_file_content(2);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().kind(), io::ErrorKind::InvalidData);
}

// -----------------------------------------------------------------------
// block directory fixture: v2 header (16B), "X2B" magic, EXTENTS inode
// -----------------------------------------------------------------------

/// Build a block-format directory buffer (v2, 16-byte header) with
/// synthetic entries that would be stored in extent-backed data blocks.
/// The inode itself is not part of this buffer — this is the raw data
/// that `read_extent_data` would return.
fn build_block_dir_v2_data() -> Vec<u8> {
    let hdr_size = XFS_DIR2_DATA_HDR_SIZE;
    let block_size: usize = 512;
    let mut buf = vec![0u8; block_size];

    buf[0..4].copy_from_slice(&XFS_DIR2_BLOCK_MAGIC.to_be_bytes());

    // Entry 1: "file1.txt" → inode 100, ftype=REG(1)
    let e1_inumber = 100u64;
    let e1_name = b"file1.txt";
    let e1_namelen = e1_name.len() as u8;
    let mut pos = hdr_size;
    buf[pos..pos + 8].copy_from_slice(&e1_inumber.to_be_bytes());
    buf[pos + 8] = e1_namelen;
    buf[pos + 9..pos + 9 + e1_name.len()].copy_from_slice(e1_name);
    let e1_ftype_pos = pos + 9 + e1_name.len();
    buf[e1_ftype_pos] = XFS_DIR3_FT_REG_FILE;
    let e1_end = e1_ftype_pos + 3;
    let e1_padded =
        e1_end + ((XFS_DIR2_DATA_ALIGN - (e1_end % XFS_DIR2_DATA_ALIGN)) % XFS_DIR2_DATA_ALIGN);
    buf[e1_padded - 2..e1_padded].copy_from_slice(&(pos as u16).to_be_bytes());
    pos = e1_end + ((-(e1_end as isize as i64)) & 7) as usize;

    // Entry 2: "subdir" → inode 200, ftype=DIR(2)
    let e2_inumber = 200u64;
    let e2_name = b"subdir";
    let e2_namelen = e2_name.len() as u8;
    buf[pos..pos + 8].copy_from_slice(&e2_inumber.to_be_bytes());
    buf[pos + 8] = e2_namelen;
    buf[pos + 9..pos + 9 + e2_name.len()].copy_from_slice(e2_name);
    let e2_ftype_pos = pos + 9 + e2_name.len();
    buf[e2_ftype_pos] = XFS_DIR3_FT_DIR;
    let e2_end = e2_ftype_pos + 3;
    let e2_padded =
        e2_end + ((XFS_DIR2_DATA_ALIGN - (e2_end % XFS_DIR2_DATA_ALIGN)) % XFS_DIR2_DATA_ALIGN);
    buf[e2_padded - 2..e2_padded].copy_from_slice(&(pos as u16).to_be_bytes());

    buf
}

fn build_data_dir_v2_data() -> Vec<u8> {
    let hdr_size = XFS_DIR2_DATA_HDR_SIZE;
    let block_size: usize = 512;
    let mut buf = vec![0u8; block_size];

    buf[0..4].copy_from_slice(&XFS_DIR2_DATA_MAGIC.to_be_bytes());
    write_xdd3_entry(
        &mut buf,
        hdr_size,
        0x0100_0003,
        b"from-16-byte-hdr",
        Some(XFS_DIR3_FT_REG_FILE),
    );

    buf
}

/// Build a v5 block directory buffer (64-byte header, "XDB3" magic).
fn build_block_dir_v5_data() -> Vec<u8> {
    let hdr_size = XFS_DIR3_DATA_HDR_SIZE; // 64
    let block_size: usize = 512;
    let mut buf = vec![0u8; block_size];

    // v5 block header
    buf[0..4].copy_from_slice(&XFS_DIR3_BLOCK_MAGIC.to_be_bytes());

    // Single entry: "passwd" → inode 42, ftype=REG(1)
    let inumber = 42u64;
    let name = b"passwd";
    let namelen = name.len() as u8;
    let pos = hdr_size;
    buf[pos..pos + 8].copy_from_slice(&inumber.to_be_bytes());
    buf[pos + 8] = namelen;
    buf[pos + 9..pos + 9 + name.len()].copy_from_slice(name);
    let ftype_pos = pos + 9 + name.len();
    buf[ftype_pos] = XFS_DIR3_FT_REG_FILE;
    let raw_end = ftype_pos + 3;
    let padded_end =
        raw_end + ((XFS_DIR2_DATA_ALIGN - (raw_end % XFS_DIR2_DATA_ALIGN)) % XFS_DIR2_DATA_ALIGN);
    buf[padded_end - 2..padded_end].copy_from_slice(&(pos as u16).to_be_bytes());

    buf
}

fn build_data_dir_v5_data_without_ftype() -> Vec<u8> {
    let hdr_size = XFS_DIR3_DATA_HDR_SIZE; // 64
    let block_size: usize = 512;
    let mut buf = vec![0u8; block_size];

    buf[0..4].copy_from_slice(&XFS_DIR3_DATA_MAGIC.to_be_bytes());

    let inumber = 0x0100_0001u64;
    let name = b"shadow";
    let namelen = name.len() as u8;
    let pos = hdr_size;
    buf[pos..pos + 8].copy_from_slice(&inumber.to_be_bytes());
    buf[pos + 8] = namelen;
    buf[pos + 9..pos + 9 + name.len()].copy_from_slice(name);
    let raw_end = pos + 9 + name.len() + 2;
    let padded_end =
        raw_end + ((XFS_DIR2_DATA_ALIGN - (raw_end % XFS_DIR2_DATA_ALIGN)) % XFS_DIR2_DATA_ALIGN);
    let tag_pos = padded_end - 2;
    buf[tag_pos..tag_pos + 2].copy_from_slice(&(pos as u16).to_be_bytes());

    buf
}

fn build_data_dir_v5_data_with_ftype() -> Vec<u8> {
    let hdr_size = XFS_DIR3_DATA_HDR_SIZE; // 64
    let block_size: usize = 512;
    let mut buf = vec![0u8; block_size];

    buf[0..4].copy_from_slice(&XFS_DIR3_DATA_MAGIC.to_be_bytes());

    let inumber = 0x0100_0002u64;
    let name = b"systemd";
    let namelen = name.len() as u8;
    let pos = hdr_size;
    buf[pos..pos + 8].copy_from_slice(&inumber.to_be_bytes());
    buf[pos + 8] = namelen;
    buf[pos + 9..pos + 9 + name.len()].copy_from_slice(name);
    let ftype_pos = pos + 9 + name.len();
    buf[ftype_pos] = XFS_DIR3_FT_DIR;
    let raw_end = ftype_pos + 3;
    let padded_end =
        raw_end + ((XFS_DIR2_DATA_ALIGN - (raw_end % XFS_DIR2_DATA_ALIGN)) % XFS_DIR2_DATA_ALIGN);
    buf[padded_end - 2..padded_end].copy_from_slice(&(pos as u16).to_be_bytes());

    buf
}

fn build_data_dir_v5_multi_entry_with_ftype() -> Vec<u8> {
    let mut buf = vec![0u8; 512];
    buf[0..4].copy_from_slice(&XFS_DIR3_DATA_MAGIC.to_be_bytes());

    let mut pos = XFS_DIR3_DATA_HDR_SIZE;
    append_xdd3_entry(
        &mut buf,
        &mut pos,
        0x0100_0010,
        b"passwd",
        Some(XFS_DIR3_FT_REG_FILE),
    );
    append_xdd3_entry(
        &mut buf,
        &mut pos,
        0x0100_0011,
        b"systemd",
        Some(XFS_DIR3_FT_DIR),
    );
    append_xdd3_entry(
        &mut buf,
        &mut pos,
        0x0100_0012,
        b"hostname",
        Some(XFS_DIR3_FT_REG_FILE),
    );

    buf
}

fn build_data_dir_v5_entry_with_alignment_sensitive_tag() -> Vec<u8> {
    let mut buf = vec![0u8; 512];
    buf[0..4].copy_from_slice(&XFS_DIR3_DATA_MAGIC.to_be_bytes());
    write_xdd3_entry(
        &mut buf,
        XFS_DIR3_DATA_HDR_SIZE,
        0x0100_0020,
        b"abcde",
        Some(XFS_DIR3_FT_REG_FILE),
    );
    buf
}

fn build_data_dir_v5_multi_entry_without_ftype() -> Vec<u8> {
    let mut buf = vec![0u8; 512];
    buf[0..4].copy_from_slice(&XFS_DIR3_DATA_MAGIC.to_be_bytes());

    let mut pos = XFS_DIR3_DATA_HDR_SIZE;
    append_xdd3_entry(&mut buf, &mut pos, 0x0100_0030, b"shadow", None);
    append_xdd3_entry(&mut buf, &mut pos, 0x0100_0031, b"group", None);

    buf
}

fn write_xdd3_entry(buf: &mut [u8], pos: usize, inumber: u64, name: &[u8], ftype: Option<u8>) {
    buf[pos..pos + 8].copy_from_slice(&inumber.to_be_bytes());
    buf[pos + 8] = name.len() as u8;
    buf[pos + 9..pos + 9 + name.len()].copy_from_slice(name);
    let record_len = 9 + name.len() + usize::from(ftype.is_some()) + 2;
    let padded_end = pos
        + record_len
        + ((XFS_DIR2_DATA_ALIGN - (record_len % XFS_DIR2_DATA_ALIGN)) % XFS_DIR2_DATA_ALIGN);
    let tag_pos = padded_end - 2;
    if let Some(ftype) = ftype {
        buf[pos + 9 + name.len()] = ftype;
    }
    buf[tag_pos..tag_pos + 2].copy_from_slice(&(pos as u16).to_be_bytes());
}

fn write_xdd3_unused(buf: &mut [u8], pos: usize, record_len: usize) {
    buf[pos..pos + 2].copy_from_slice(&XFS_DIR2_FREE_TAG.to_be_bytes());
    buf[pos + 2..pos + 4].copy_from_slice(&(record_len as u16).to_be_bytes());
    buf[pos + record_len - 2..pos + record_len].copy_from_slice(&(pos as u16).to_be_bytes());
}

fn append_xdd3_entry(
    buf: &mut [u8],
    pos: &mut usize,
    inumber: u64,
    name: &[u8],
    ftype: Option<u8>,
) {
    write_xdd3_entry(buf, *pos, inumber, name, ftype);
    let record_len = 9 + name.len() + usize::from(ftype.is_some()) + 2;
    *pos += record_len
        + ((XFS_DIR2_DATA_ALIGN - (record_len % XFS_DIR2_DATA_ALIGN)) % XFS_DIR2_DATA_ALIGN);
}

fn build_xfs_fixture_with_xdd3_extent_dir() -> Vec<u8> {
    let mut img = build_xfs_fixture();
    let block_size = 4096usize;

    let fi = &mut img[8192 + 2 * 256..8192 + 3 * 256];
    fi[di_off::MODE..di_off::MODE + 2].copy_from_slice(&(S_IFDIR | 0o755).to_be_bytes());
    fi[di_off::FORMAT] = FORMAT_EXTENTS;
    fi[di_off::SIZE..di_off::SIZE + 8].copy_from_slice(&(block_size as u64).to_be_bytes());
    fi[di_off::NEXTENTS..di_off::NEXTENTS + 4].copy_from_slice(&1u32.to_be_bytes());
    fi[di_off::FORKOFF] = 0;
    fi[INODE_CORE_SIZE..INODE_CORE_SIZE + 16].copy_from_slice(&encode_bmbt_extent(0, 7, 1));

    let block7 = 7 * block_size;
    img[block7..block7 + block_size].fill(0);
    let dir = &mut img[block7..block7 + block_size];
    dir[0..4].copy_from_slice(&XFS_DIR3_DATA_MAGIC.to_be_bytes());
    write_xdd3_entry(dir, XFS_DIR3_DATA_HDR_SIZE, 4, b"subdir", None);

    img
}

fn build_xfs_fixture_with_valid_empty_xdd3_dir_and_residual_shortform() -> Vec<u8> {
    let mut img = build_xfs_fixture_with_zeroed_block_dir_and_residual_shortform();
    let block_size = 4096usize;
    let block7 = 7 * block_size;

    img[block7..block7 + block_size].fill(0);
    img[block7..block7 + 4].copy_from_slice(&XFS_DIR3_DATA_MAGIC.to_be_bytes());

    img
}

fn build_xfs_fixture_with_valid_empty_xdd2_dir_and_residual_shortform() -> Vec<u8> {
    let mut img = build_xfs_fixture_with_zeroed_block_dir_and_residual_shortform();
    let block_size = 4096usize;
    let block7 = 7 * block_size;

    img[block7..block7 + block_size].fill(0);
    img[block7..block7 + 4].copy_from_slice(&XFS_DIR2_DATA_MAGIC.to_be_bytes());

    img
}

fn build_xfs_fixture_with_truncated_second_xdd3_block() -> Vec<u8> {
    let mut img = build_xfs_fixture_with_xdd3_extent_dir();
    let block_size = 4096usize;

    let fi = &mut img[8192 + 2 * 256..8192 + 3 * 256];
    fi[di_off::SIZE..di_off::SIZE + 8].copy_from_slice(&(2 * block_size as u64).to_be_bytes());
    fi[INODE_CORE_SIZE..INODE_CORE_SIZE + 16].copy_from_slice(&encode_bmbt_extent(0, 7, 2));

    img.truncate(8 * block_size);
    img
}

fn build_xfs_fixture_with_multi_fsb_directory_block() -> Vec<u8> {
    let mut img = build_xfs_fixture();
    let block_size = 4096usize;
    img[sb_off::DIRBLKLOG] = 1;

    let fi = &mut img[8192 + 2 * 256..8192 + 3 * 256];
    fi[di_off::MODE..di_off::MODE + 2].copy_from_slice(&(S_IFDIR | 0o755).to_be_bytes());
    fi[di_off::FORMAT] = FORMAT_EXTENTS;
    fi[di_off::SIZE..di_off::SIZE + 8].copy_from_slice(&(2 * block_size as u64).to_be_bytes());
    fi[di_off::NEXTENTS..di_off::NEXTENTS + 4].copy_from_slice(&1u32.to_be_bytes());
    fi[di_off::FORKOFF] = 0;
    fi[INODE_CORE_SIZE..INODE_CORE_SIZE + 16].copy_from_slice(&encode_bmbt_extent(0, 7, 2));

    let block7 = 7 * block_size;
    img[block7..block7 + 2 * block_size].fill(0);
    let dir = &mut img[block7..block7 + 2 * block_size];
    dir[0..4].copy_from_slice(&XFS_DIR3_DATA_MAGIC.to_be_bytes());
    write_xdd3_unused(dir, XFS_DIR3_DATA_HDR_SIZE, block_size);
    let pos = block_size + XFS_DIR3_DATA_HDR_SIZE;
    write_xdd3_entry(dir, pos, 4, b"subdir", None);

    img
}

fn build_xfs_fixture_with_bad_first_block_and_valid_later_xdd3_block() -> Vec<u8> {
    let mut img = build_xfs_fixture();
    let block_size = 4096usize;

    let fi = &mut img[8192 + 2 * 256..8192 + 3 * 256];
    fi[di_off::MODE..di_off::MODE + 2].copy_from_slice(&(S_IFDIR | 0o755).to_be_bytes());
    fi[di_off::FORMAT] = FORMAT_EXTENTS;
    fi[di_off::SIZE..di_off::SIZE + 8].copy_from_slice(&(2 * block_size as u64).to_be_bytes());
    fi[di_off::NEXTENTS..di_off::NEXTENTS + 4].copy_from_slice(&2u32.to_be_bytes());
    fi[di_off::FORKOFF] = 0;
    fi[INODE_CORE_SIZE..INODE_CORE_SIZE + 16].copy_from_slice(&encode_bmbt_extent(0, 7, 1));
    fi[INODE_CORE_SIZE + 16..INODE_CORE_SIZE + 32].copy_from_slice(&encode_bmbt_extent(1, 8, 1));

    let block7 = 7 * block_size;
    img[block7..block7 + block_size].fill(0);
    img[block7..block7 + 4].copy_from_slice(b"BAD!");

    let block8 = 8 * block_size;
    img[block8..block8 + block_size].fill(0);
    let dir = &mut img[block8..block8 + block_size];
    dir[0..4].copy_from_slice(&XFS_DIR3_DATA_MAGIC.to_be_bytes());
    write_xdd3_entry(dir, XFS_DIR3_DATA_HDR_SIZE, 4, b"subdir", None);

    img
}
fn build_xfs_fixture_with_zeroed_block_dir_and_v5_ftype_residual_shortform() -> Vec<u8> {
    let mut img = build_xfs_fixture();
    let block_size = 4096u64;

    let sb = &mut img[0..512];
    sb[sb_off::FEATURES_INCOMPAT..sb_off::FEATURES_INCOMPAT + 4]
        .copy_from_slice(&XFS_SB_FEAT_INCOMPAT_FTYPE.to_be_bytes());

    let root = &mut img[8192 + 256..8192 + 2 * 256];
    let root_df = &mut root[INODE_CORE_SIZE..];
    root_df.fill(0);
    root_df[0] = 2;
    root_df[1] = 2;
    root_df[2..10].copy_from_slice(&2u64.to_be_bytes());
    let mut root_pos = DIR2_SF_HDR_8;
    root_df[root_pos] = 8;
    root_pos += 1;
    root_df[root_pos..root_pos + 2].copy_from_slice(&0x0018u16.to_be_bytes());
    root_pos += 2;
    root_df[root_pos..root_pos + 8].copy_from_slice(b"test.txt");
    root_pos += 8;
    root_df[root_pos] = XFS_DIR3_FT_DIR;
    root_pos += 1;
    root_df[root_pos..root_pos + 8].copy_from_slice(&3u64.to_be_bytes());
    root_pos += 8;
    root_df[root_pos] = 6;
    root_pos += 1;
    root_df[root_pos..root_pos + 2].copy_from_slice(&0x0040u16.to_be_bytes());
    root_pos += 2;
    root_df[root_pos..root_pos + 6].copy_from_slice(b"subdir");
    root_pos += 6;
    root_df[root_pos] = XFS_DIR3_FT_DIR;
    root_pos += 1;
    root_df[root_pos..root_pos + 8].copy_from_slice(&4u64.to_be_bytes());

    let fi = &mut img[8192 + 2 * 256..8192 + 3 * 256];
    fi[di_off::MODE..di_off::MODE + 2].copy_from_slice(&(S_IFDIR | 0o755).to_be_bytes());
    fi[di_off::FORMAT] = FORMAT_EXTENTS;
    fi[di_off::SIZE..di_off::SIZE + 8].copy_from_slice(&block_size.to_be_bytes());
    fi[di_off::NEXTENTS..di_off::NEXTENTS + 4].copy_from_slice(&1u32.to_be_bytes());
    fi[di_off::FORKOFF] = 2;

    let core = INODE_CORE_SIZE;
    fi[core..core + 16].copy_from_slice(&encode_bmbt_extent(0, 7, 1));

    let residual = &mut fi[core + 16..];
    residual[0] = 1;
    residual[1] = 1;
    residual[2..10].copy_from_slice(&2u64.to_be_bytes());
    let mut pos = DIR2_SF_HDR_8;
    residual[pos] = 6;
    pos += 1;
    residual[pos..pos + 2].copy_from_slice(&0x0018u16.to_be_bytes());
    pos += 2;
    residual[pos..pos + 6].copy_from_slice(b"subdir");
    pos += 6;
    residual[pos] = XFS_DIR3_FT_DIR;
    pos += 1;
    residual[pos..pos + 8].copy_from_slice(&4u64.to_be_bytes());

    let block7 = 7usize * block_size as usize;
    img[block7..block7 + block_size as usize].fill(0);
    img
}

fn build_xfs_fixture_with_btree_xdd3_directory_and_truncated_second_block() -> Vec<u8> {
    let mut img = build_xfs_fixture_with_xdd3_extent_dir();
    let block_size = 4096usize;

    let fi = &mut img[8192 + 2 * 256..8192 + 3 * 256];
    fi[di_off::FORMAT] = FORMAT_BTREE;
    fi[di_off::SIZE..di_off::SIZE + 8].copy_from_slice(&(2 * block_size as u64).to_be_bytes());
    fi[di_off::NEXTENTS..di_off::NEXTENTS + 4].copy_from_slice(&2u32.to_be_bytes());
    fi[di_off::FORKOFF] = 0;

    let df = &mut fi[INODE_CORE_SIZE..];
    df[0..4].copy_from_slice(&BMAP_MAGIC.to_be_bytes());
    df[4..6].copy_from_slice(&0u16.to_be_bytes());
    df[6..8].copy_from_slice(&2u16.to_be_bytes());
    df[8..16].copy_from_slice(&0u64.to_be_bytes());
    df[16..32].copy_from_slice(&encode_bmbt_extent(0, 7, 1));
    df[32..40].copy_from_slice(&1u64.to_be_bytes());
    df[40..56].copy_from_slice(&encode_bmbt_extent(1, 8, 1));

    img.truncate(8 * block_size);
    img
}

/// Build a block directory buffer with a free-space (0xFFFF) entry
/// interleaved between two entries.
fn build_block_dir_data_with_free_space() -> Vec<u8> {
    let hdr_size = XFS_DIR2_DATA_HDR_SIZE;
    let block_size: usize = 512;
    let mut buf = vec![0u8; block_size];

    buf[0..4].copy_from_slice(&XFS_DIR2_BLOCK_MAGIC.to_be_bytes());

    // Entry 1: "good.txt" → inode 10, REG
    let inumber1 = 10u64;
    let name1 = b"good.txt";
    let namelen1 = name1.len() as u8;
    let mut pos = hdr_size;
    buf[pos..pos + 8].copy_from_slice(&inumber1.to_be_bytes());
    buf[pos + 8] = namelen1;
    buf[pos + 9..pos + 9 + name1.len()].copy_from_slice(name1);
    let ft1 = pos + 9 + name1.len();
    buf[ft1] = XFS_DIR3_FT_REG_FILE;
    let e1_end = ft1 + 3;
    let e1_padded =
        e1_end + ((XFS_DIR2_DATA_ALIGN - (e1_end % XFS_DIR2_DATA_ALIGN)) % XFS_DIR2_DATA_ALIGN);
    buf[e1_padded - 2..e1_padded].copy_from_slice(&(pos as u16).to_be_bytes());
    let p1 = e1_end + ((-(e1_end as isize as i64)) & 7) as usize;

    // Free-space record spanning 32 bytes
    let free_len: u16 = 32;
    write_xdd3_unused(&mut buf, p1, free_len as usize);
    pos = p1 + free_len as usize;

    // Entry 2: "keep.txt" → inode 20, DIR
    let inumber2 = 20u64;
    let name2 = b"keep.txt";
    let namelen2 = name2.len() as u8;
    buf[pos..pos + 8].copy_from_slice(&inumber2.to_be_bytes());
    buf[pos + 8] = namelen2;
    buf[pos + 9..pos + 9 + name2.len()].copy_from_slice(name2);
    let ft2 = pos + 9 + name2.len();
    buf[ft2] = XFS_DIR3_FT_DIR;
    let e2_end = ft2 + 3;
    let e2_padded =
        e2_end + ((XFS_DIR2_DATA_ALIGN - (e2_end % XFS_DIR2_DATA_ALIGN)) % XFS_DIR2_DATA_ALIGN);
    buf[e2_padded - 2..e2_padded].copy_from_slice(&(pos as u16).to_be_bytes());
    let _p2 = e2_end + ((-(e2_end as isize as i64)) & 7) as usize;

    buf
}

// -----------------------------------------------------------------------
// test_parse_block_dir_v2
// -----------------------------------------------------------------------

#[test]
fn test_parse_block_dir_v2_starts_entries_after_16_byte_header() {
    let data = build_block_dir_v2_data();
    let entries = parse_block_dir(&data).unwrap();
    assert_eq!(entries.len(), 2, "v2 block should produce 2 entries");

    let file1 = entries.iter().find(|(n, _, _)| n == "file1.txt").unwrap();
    assert_eq!(file1.1, 100);
    assert!(!file1.2, "ftype=1 → not a directory");

    let sub = entries.iter().find(|(n, _, _)| n == "subdir").unwrap();
    assert_eq!(sub.1, 200);
    assert!(sub.2, "ftype=2 → is a directory");
}

#[test]
fn test_parse_data_dir_v2_starts_entries_after_16_byte_header() {
    let data = build_data_dir_v2_data();
    let entries = parse_block_dir(&data).unwrap();

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].0, "from-16-byte-hdr");
    assert_eq!(entries[0].1, 0x0100_0003);
}

// -----------------------------------------------------------------------
// test_parse_block_dir_v5
// -----------------------------------------------------------------------

#[test]
fn test_parse_block_dir_v5() {
    let data = build_block_dir_v5_data();
    let entries = parse_block_dir(&data).unwrap();
    assert_eq!(entries.len(), 1, "v5 block should produce 1 entry");
    assert_eq!(entries[0].0, "passwd");
    assert_eq!(entries[0].1, 42);
    assert!(!entries[0].2, "ftype=1 → file");
}

#[test]
fn test_parse_data_dir_v5_without_ftype() {
    let data = build_data_dir_v5_data_without_ftype();
    let entries = parse_block_dir(&data).unwrap();

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].0, "shadow");
    assert_eq!(entries[0].1, 0x0100_0001);
    assert!(
        !entries[0].2,
        "no-ftype XDD3 entries require child inode annotation"
    );

    let raw = parse_block_dir_entries(&data).unwrap();
    assert_eq!(raw[0].ftype, None);
}

#[test]
fn test_parse_data_dir_v5_with_ftype() {
    let data = build_data_dir_v5_data_with_ftype();
    let entries = parse_block_dir(&data).unwrap();

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].0, "systemd");
    assert_eq!(entries[0].1, 0x0100_0002);
    assert!(entries[0].2);

    let raw = parse_block_dir_entries(&data).unwrap();
    assert_eq!(raw[0].ftype, Some(XFS_DIR3_FT_DIR));
}

#[test]
fn test_parse_data_dir_v5_tag_is_at_end_of_8_byte_aligned_record() {
    let data = build_data_dir_v5_entry_with_alignment_sensitive_tag();
    let entries = parse_block_dir_entries(&data).unwrap();

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].name, "abcde");
    assert_eq!(entries[0].inode, 0x0100_0020);
    assert_eq!(entries[0].ftype, Some(XFS_DIR3_FT_REG_FILE));
}

#[test]
fn test_parse_data_dir_v5_multi_entry_block_with_ftype() {
    let data = build_data_dir_v5_multi_entry_with_ftype();
    let entries = parse_block_dir_entries(&data).unwrap();
    let names = entries
        .iter()
        .map(|entry| (entry.name.as_str(), entry.ftype))
        .collect::<Vec<_>>();

    assert_eq!(
        names,
        vec![
            ("passwd", Some(XFS_DIR3_FT_REG_FILE)),
            ("systemd", Some(XFS_DIR3_FT_DIR)),
            ("hostname", Some(XFS_DIR3_FT_REG_FILE)),
        ]
    );
}

#[test]
fn test_parse_data_dir_v5_multi_entry_block_without_ftype() {
    let data = build_data_dir_v5_multi_entry_without_ftype();
    let entries = parse_block_dir_entries(&data).unwrap();
    let names = entries
        .iter()
        .map(|entry| (entry.name.as_str(), entry.ftype))
        .collect::<Vec<_>>();

    assert_eq!(names, vec![("shadow", None), ("group", None)]);
}

#[test]
fn test_extent_directory_accepts_xdd3_data_block_and_annotates_child_inode() {
    let img = build_xfs_fixture_with_xdd3_extent_dir();
    let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
    let xfs = XfsReader::open(reader, 0).unwrap();

    let children = xfs.list_children("test.txt").unwrap();

    assert_eq!(children.len(), 1);
    assert_eq!(children[0].name, "subdir");
    assert!(children[0].is_dir);
}

#[test]
fn test_extent_directory_keeps_valid_xdd3_entries_when_later_block_is_truncated() {
    let img = build_xfs_fixture_with_truncated_second_xdd3_block();
    let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
    let xfs = XfsReader::open(reader, 0).unwrap();

    let children = xfs.list_children("test.txt").unwrap();

    assert_eq!(children.len(), 1);
    assert_eq!(children[0].name, "subdir");
    assert!(children[0].is_dir);
}

#[test]
fn test_btree_directory_keeps_valid_xdd3_entries_when_later_block_is_truncated() {
    let img = build_xfs_fixture_with_btree_xdd3_directory_and_truncated_second_block();
    let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
    let xfs = XfsReader::open(reader, 0).unwrap();

    let children = xfs.list_children("test.txt").unwrap();

    assert_eq!(children.len(), 1);
    assert_eq!(children[0].name, "subdir");
    assert!(children[0].is_dir);
}

// -----------------------------------------------------------------------
// test_parse_block_dir_with_free_space
// -----------------------------------------------------------------------

#[test]
fn test_parse_block_dir_with_free_space() {
    let data = build_block_dir_data_with_free_space();
    let entries = parse_block_dir(&data).unwrap();
    assert_eq!(entries.len(), 2, "free-space entry should be skipped");
    assert!(entries.iter().any(|(n, _, _)| n == "good.txt"));
    assert!(entries.iter().any(|(n, _, _)| n == "keep.txt"));
}

#[test]
fn test_parse_block_dir_rejects_unaligned_free_space_record() {
    let mut data = vec![0u8; 512];
    data[0..4].copy_from_slice(&XFS_DIR2_BLOCK_MAGIC.to_be_bytes());
    let pos = XFS_DIR2_DATA_HDR_SIZE;
    write_xdd3_unused(&mut data, pos, 10);
    write_xdd3_entry(
        &mut data,
        pos + 10,
        20,
        b"bad-after-free",
        Some(XFS_DIR3_FT_REG_FILE),
    );

    let entries = parse_block_dir(&data).unwrap();

    assert!(
        entries.is_empty(),
        "unaligned free-space records must not advance into a bogus entry"
    );
}

#[test]
fn test_parse_block_dir_rejects_free_space_record_with_bad_tag() {
    let mut data = vec![0u8; 512];
    data[0..4].copy_from_slice(&XFS_DIR2_BLOCK_MAGIC.to_be_bytes());
    let pos = XFS_DIR2_DATA_HDR_SIZE;
    write_xdd3_unused(&mut data, pos, 16);
    data[pos + 14..pos + 16].copy_from_slice(&0x1234u16.to_be_bytes());
    write_xdd3_entry(
        &mut data,
        pos + 16,
        20,
        b"bad-tag",
        Some(XFS_DIR3_FT_REG_FILE),
    );

    let entries = parse_block_dir(&data).unwrap();

    assert!(
        entries.is_empty(),
        "free-space records with mismatched tail tags must be rejected"
    );
}

#[test]
fn test_parse_block_dir_rejects_unaligned_active_entry_start() {
    let mut data = vec![0u8; 512];
    data[0..4].copy_from_slice(&XFS_DIR2_BLOCK_MAGIC.to_be_bytes());
    let pos = XFS_DIR2_DATA_HDR_SIZE + 1;
    write_xdd3_entry(&mut data, pos, 30, b"unaligned", Some(XFS_DIR3_FT_REG_FILE));

    let entries = parse_block_dir(&data).unwrap();

    assert!(
        entries.is_empty(),
        "active directory entries must start on XFS 8-byte boundaries"
    );
}

// -----------------------------------------------------------------------
// test_parse_block_dir_empty_data
// -----------------------------------------------------------------------

#[test]
fn test_parse_block_dir_empty_data() {
    let entries = parse_block_dir(&[]).unwrap();
    assert!(entries.is_empty());
}

// -----------------------------------------------------------------------
// test_parse_block_dir_unknown_magic_errors
// -----------------------------------------------------------------------

#[test]
fn test_parse_block_dir_unknown_magic_errors() {
    let mut data = vec![0u8; 64];
    data[0..4].copy_from_slice(b"BAD!");
    let result = parse_block_dir(&data);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().kind(), io::ErrorKind::InvalidData);
}

#[test]
fn test_valid_empty_xdd3_dir_does_not_recover_residual_shortform() {
    let img = build_xfs_fixture_with_valid_empty_xdd3_dir_and_residual_shortform();
    let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
    let xfs = XfsReader::open(reader, 0).unwrap();

    let children = xfs.list_children("test.txt").unwrap();

    assert!(children.is_empty());
}

#[test]
fn test_valid_empty_xdd2_dir_does_not_recover_residual_shortform() {
    let img = build_xfs_fixture_with_valid_empty_xdd2_dir_and_residual_shortform();
    let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
    let xfs = XfsReader::open(reader, 0).unwrap();

    let children = xfs.list_children("test.txt").unwrap();

    assert!(children.is_empty());
}

#[test]
fn test_zeroed_block_dir_recovers_residual_shortform_entries() {
    let img = build_xfs_fixture_with_zeroed_block_dir_and_residual_shortform();
    let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
    let xfs = XfsReader::open(reader, 0).unwrap();

    let children = xfs.list_children("test.txt").unwrap();

    assert_eq!(children.len(), 1);
    assert_eq!(children[0].name, "subdir");
    assert!(children[0].is_dir);
}

#[test]
fn test_zeroed_block_dir_without_residual_shortform_returns_invalid_data() {
    let img = build_xfs_fixture_with_zeroed_block_dir_without_residual_shortform();
    let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
    let xfs = XfsReader::open(reader, 0).unwrap();

    let err = xfs.list_children("test.txt").unwrap_err();

    assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    assert!(
        err.to_string().contains("zeroed block directory data")
            || err.to_string().contains("recovery failed")
    );
}

#[test]
fn test_partial_directory_metadata_read_can_use_later_valid_block() {
    let img = build_xfs_fixture_with_bad_first_block_and_valid_later_xdd3_block();
    let block_size = 4096usize;
    let reader: Box<dyn EvidenceReader> = Box::new(PartialRangeReader::new(
        img,
        7 * block_size + 4,
        8 * block_size,
    ));
    let xfs = XfsReader::open(reader, 0).unwrap();

    let children = xfs.list_children("test.txt").unwrap();

    assert_eq!(children.len(), 1);
    assert_eq!(children[0].name, "subdir");
    assert!(children[0].is_dir);
}

#[test]
fn test_zeroed_block_dir_recovers_v5_ftype_residual_shortform_entries() {
    let img = build_xfs_fixture_with_zeroed_block_dir_and_v5_ftype_residual_shortform();
    let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
    let xfs = XfsReader::open(reader, 0).unwrap();

    let children = xfs.list_children("test.txt").unwrap();

    assert_eq!(children.len(), 1);
    assert_eq!(children[0].name, "subdir");
    assert!(children[0].is_dir);
}

#[test]
fn test_multi_fsb_directory_block_is_read_as_one_directory_block() {
    let img = build_xfs_fixture_with_multi_fsb_directory_block();
    let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
    let xfs = XfsReader::open(reader, 0).unwrap();

    let children = xfs.list_children("test.txt").unwrap();

    assert_eq!(children.len(), 1);
    assert_eq!(children[0].name, "subdir");
    assert!(children[0].is_dir);
}
