//! XFS filesystem reader.
//!
//! Implements the `FileSystemReader` trait for XFS-formatted volumes.
//! Parses the superblock at offset 0 (magic `XFSB`), allocation group
//! geometry, inode core structures, shortform directories, extent-mapped
//! files, and B+tree extent maps.
//!
//! All on-disk multi-byte integer fields are big-endian (the XFS canonical
//! format).
//!
//! Supported features:
//! - v2 / v3 superblock with sb_blocksize, sb_agcount, sb_inodesize, sb_inopblock
//! - v2 inode core (96 bytes): di_magic, di_mode, di_format, di_size, di_forkoff
//! - Shortform directories (di_format = 1): inline xfs_dir2_sf_hdr + entries
//! - Extent-mapped files (di_format = 2): bmbt records in data fork
//! - B+tree extent maps (di_format = 3): bmbt btree blocks with leaf records
//! - Log replay and metadata/deleted-inode recovery (`log` module)

pub mod log;

use evidence_core::filesystem::{
    child_nodes_with_parent_path, file_not_found, fs_node_without_timestamps, invalid_fs_data,
    is_special_directory_name, path_components, path_is_directory, path_is_not_directory,
    path_not_found, root_node, truncate_data_to_declared_size, FileSystemReader, FsNode,
};
use evidence_core::EvidenceReader;
use std::cell::RefCell;
use std::io::{self, Read, Seek, SeekFrom};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Superblock magic: `XFSB` → 0x58465342 (big-endian).
const XFS_SUPER_MAGIC: u32 = 0x5846_5342;

/// Inode magic: `IN` → 0x494E (big-endian).
const XFS_INODE_MAGIC: u16 = 0x494E;

/// B+tree block-map magic: `BMAP` → 0x424D4150 (big-endian).
const BMAP_MAGIC: u32 = 0x424D_4150;

/// Standard Unix inode type masks.
const S_IFDIR: u16 = 0x4000;
// Used by xfs tests; format constant.
#[cfg(test)]
const S_IFREG: u16 = 0x8000;

/// di_format values (inode data-fork layout).
const FORMAT_LOCAL: u8 = 1;
const FORMAT_EXTENTS: u8 = 2;
const FORMAT_BTREE: u8 = 3;

/// Size of the v2 inode core in bytes.  The data fork starts immediately
/// after the core inside the inode buffer.
/// For v3/v5 inodes the core is 176 bytes; this is detected dynamically
/// when the inode buffer is processed.
const INODE_CORE_SIZE: usize = 96;
/// Size of the v3/v5 inode core (includes the v2 core + 80 bytes of v3
/// extended fields).
const INODE_CORE_SIZE_V3: usize = 176;

/// Size of one B+tree block-map record (two big-endian u64s).
const BMBT_REC_SIZE: usize = 16;

/// Base size of the shortform-directory header (i8count > 0):
/// count(1) + i8count(1) + parent-inode(8) = 10.
/// When i8count == 0, parent is 4 bytes → header = 6 bytes.
const DIR2_SF_HDR_8: usize = 10;
const DIR2_SF_HDR_4: usize = 6;

// ---------------------------------------------------------------------------
// Block directory constants (xfs_dir3_data_hdr / xfs_dir2_data_hdr)
// ---------------------------------------------------------------------------

/// v5 block directory magic "XDB3" = 0x58444233 (big-endian).
const XFS_DIR3_BLOCK_MAGIC: u32 = 0x5844_4233;
/// v3/v4 block directory magic "XDB2" = 0x58444232 (big-endian).
const XFS_DIR2_BLOCK_MAGIC: u32 = 0x5844_4232;
/// v5 block data header size in bytes.
const XFS_DIR3_DATA_HDR_SIZE: usize = 64;
/// v3/v4 block data header size in bytes.
const XFS_DIR2_DATA_HDR_SIZE: usize = 32;
/// Freetag value for xfs_dir2_data_unused entries.
const XFS_DIR2_FREE_TAG: u16 = 0xFFFF;

/// ftype values (block directory file-type tag).
#[allow(dead_code)]
const XFS_DIR3_FT_REG_FILE: u8 = 1;
const XFS_DIR3_FT_DIR: u8 = 2;

/// Key field offsets within the XFS superblock (big-endian).
///
/// Inode numbers in XFS directly *encode* their location: the high bits
/// are the AG number, the middle bits are the AG-relative block number,
/// and the low bits are the inode's index within that block. No B+tree
/// traversal is needed to resolve an inode by number (the inode B+tree
/// -- inobt -- exists purely for allocation bookkeeping: tracking which
/// inodes in a chunk are free/used). This matches SleuthKit's
/// `xfs_inode_get_offset` (tsk/fs/tsk_xfs.h), which decodes the address
/// via `ag_num = ino >> (sb_agblklog + sb_inopblog)`, etc. `sb_agblklog`
/// and `sb_inopblog` are the on-disk log2 values, not derived at runtime.
mod sb_off {
    pub const MAGIC: usize = 0x00; // u32
    pub const BLOCKSIZE: usize = 0x04; // u32
    pub const DBLOCKS: usize = 0x08; // u64
    pub const ROOTINO: usize = 0x38; // u64
    pub const AGBLOCKS: usize = 0x54; // u32
    pub const AGCOUNT: usize = 0x58; // u32
    pub const _SECTSIZE: usize = 0x66; // u16
    pub const INODESIZE: usize = 0x68; // u16
    pub const INOPBLOCK: usize = 0x6A; // u16
                                       // sb_fname[12] occupies 0x6C..0x78.
    pub const _BLOCKLOG: usize = 0x78; // u8 — log2 of sb_blocksize
    pub const _SECTLOG: usize = 0x79; // u8 — log2 of sb_sectsize
    pub const _INODELOG: usize = 0x7A; // u8 — log2 of sb_inodesize
    pub const INOPBLOG: usize = 0x7B; // u8 — log2 of sb_inopblock
    pub const AGBLKLOG: usize = 0x7C; // u8 — log2 of sb_agblocks (rounded up)
}

/// Key field offsets within the v2 inode core (big-endian).
mod di_off {
    pub const MAGIC: usize = 0x00; // u16
    pub const MODE: usize = 0x02; // u16
    pub const VERSION: usize = 0x04; // u8 — 2 for v2, 3 for v3/v5
    pub const FORMAT: usize = 0x05; // u8
    pub const SIZE: usize = 0x38; // u64
    pub const NEXTENTS: usize = 0x4C; // u32
    pub const FORKOFF: usize = 0x52; // u8
    pub const _AFORMAT: usize = 0x53; // u8
}

// ---------------------------------------------------------------------------
// Big-endian read helpers
// ---------------------------------------------------------------------------

fn be_u16(buf: &[u8], off: usize) -> u16 {
    u16::from_be_bytes([buf[off], buf[off + 1]])
}

fn be_u32(buf: &[u8], off: usize) -> u32 {
    u32::from_be_bytes([buf[off], buf[off + 1], buf[off + 2], buf[off + 3]])
}

fn be_u64(buf: &[u8], off: usize) -> u64 {
    u64::from_be_bytes([
        buf[off],
        buf[off + 1],
        buf[off + 2],
        buf[off + 3],
        buf[off + 4],
        buf[off + 5],
        buf[off + 6],
        buf[off + 7],
    ])
}

// ---------------------------------------------------------------------------
// XfsReader
// ---------------------------------------------------------------------------

pub struct XfsReader {
    reader: RefCell<Box<dyn EvidenceReader>>,
    block_size: u64,
    _ag_blocks: u64,
    _ag_count: u32,
    inode_size: u16,
    _inopblock: u16,
    root_ino: u64,
    volume_offset: u64,
    inode_base_block: u64,
    // On-disk log2 values used to decode an inode number directly into its
    // AG/block/in-block-index (see `inode_offset`).  Zero on synthetic
    // fixtures that never populate these superblock fields; real XFS
    // filesystems always have agblklog >= 6 (XFS_MIN_AG_BLOCKS = 64).
    agblklog: u8,
    inopblog: u8,
}

impl XfsReader {
    /// Open an XFS filesystem located at `offset` within the evidence reader.
    ///
    /// Reads and validates the superblock, then derives allocation-group
    /// geometry and the inode table location.
    pub fn open(mut reader: Box<dyn EvidenceReader>, offset: u64) -> io::Result<Self> {
        // Read first 512 bytes (standard XFS sector / minimal superblock).
        reader.seek(SeekFrom::Start(offset))?;
        let mut sb_buf = [0u8; 512];
        reader.read_exact(&mut sb_buf)?;

        // Validate magic.
        let magic = be_u32(&sb_buf, sb_off::MAGIC);
        if magic != XFS_SUPER_MAGIC {
            return Err(invalid_fs_data(format!(
                "not a valid XFS filesystem (magic 0x{:08X})",
                magic
            )));
        }

        let block_size = be_u32(&sb_buf, sb_off::BLOCKSIZE) as u64;
        let dblocks = be_u64(&sb_buf, sb_off::DBLOCKS);
        let ag_count = be_u32(&sb_buf, sb_off::AGCOUNT);
        let ag_blocks_from_sb = be_u32(&sb_buf, sb_off::AGBLOCKS) as u64;

        let inode_size = be_u16(&sb_buf, sb_off::INODESIZE);
        let inopblock = be_u16(&sb_buf, sb_off::INOPBLOCK);
        let root_ino = be_u64(&sb_buf, sb_off::ROOTINO);

        // Basic sanity checks.
        if block_size == 0 || dblocks == 0 || ag_count == 0 {
            return Err(invalid_fs_data("invalid XFS superblock geometry"));
        }
        if inode_size < INODE_CORE_SIZE as u16 {
            return Err(invalid_fs_data(format!(
                "inode size {} too small (need >= {})",
                inode_size, INODE_CORE_SIZE
            )));
        }

        // Use sb_agblocks from superblock if non-zero; otherwise compute.
        let ag_blocks = if ag_blocks_from_sb != 0 {
            ag_blocks_from_sb
        } else {
            dblocks / ag_count as u64
        };

        let agblklog = sb_buf[sb_off::AGBLKLOG];
        let inopblog = sb_buf[sb_off::INOPBLOG];

        // Place the inode table at a known offset for synthetic fixtures:
        // start at block 2 (skipping superblock + metadata gap).  Real XFS
        // filesystems resolve inode offsets by direct bit-decode instead
        // (see `inode_offset`); this base block is only used as a fallback.
        let inode_base_block: u64 = 2;

        Ok(Self {
            reader: RefCell::new(reader),
            block_size,
            _ag_blocks: ag_blocks,
            _ag_count: ag_count,
            inode_size,
            _inopblock: inopblock,
            root_ino,
            volume_offset: offset,
            inode_base_block,
            agblklog,
            inopblog,
        })
    }

    // ------------------------------------------------------------------
    // Internal helpers
    // ------------------------------------------------------------------

    /// Convert a filesystem-block number to a byte offset from the start of
    /// the evidence reader.
    fn block_to_offset(&self, block: u64) -> u64 {
        self.volume_offset + block * self.block_size
    }

    /// Read one full filesystem block.
    fn read_block(&self, block: u64) -> io::Result<Vec<u8>> {
        let offset = self.block_to_offset(block);
        let mut buf = vec![0u8; self.block_size as usize];
        let mut reader = self.reader.borrow_mut();
        reader.seek(SeekFrom::Start(offset))?;
        reader.read_exact(&mut buf)?;
        Ok(buf)
    }

    /// Read one full inode buffer at its physical offset.
    fn read_inode_at_offset(&self, offset: u64) -> io::Result<Vec<u8>> {
        let mut buf = vec![0u8; self.inode_size as usize];
        let mut reader = self.reader.borrow_mut();
        reader.seek(SeekFrom::Start(offset))?;
        reader.read_exact(&mut buf)?;
        Ok(buf)
    }

    /// Byte offset of the start of inode `ino` in the evidence stream.
    ///
    /// Real XFS inode numbers directly encode their location: the AG
    /// number occupies the high bits, the AG-relative block number the
    /// middle bits, and the in-block inode index the low bits (matching
    /// SleuthKit's `xfs_inode_get_offset` in tsk/fs/tsk_xfs.h). No B+tree
    /// traversal is required — the inode B+tree (inobt) tracks
    /// free/allocated inodes for allocation bookkeeping, not location.
    ///
    /// `agblklog`/`inopblog` are zero on synthetic fixtures that never
    /// populate those superblock fields, so those fixtures fall back to
    /// the flat inode-table formula they were built against.
    fn inode_offset(&self, ino: u64) -> u64 {
        if self.agblklog > 0 || self.inopblog > 0 {
            let shift = self.agblklog + self.inopblog;
            let ino0 = ino;
            let ag_num = ino0 >> shift;
            let low_bits = ino0 & ((1u64 << shift) - 1);
            let blk_num = low_bits >> self.inopblog;
            let ino_in_blk = low_bits & ((1u64 << self.inopblog) - 1);
            let fs_blockno = ag_num * self._ag_blocks + blk_num;
            return self.block_to_offset(fs_blockno) + ino_in_blk * self.inode_size as u64;
        }
        // Flat-table fallback for synthetic fixtures.
        self.block_to_offset(self.inode_base_block)
            + (ino.saturating_sub(1)) * self.inode_size as u64
    }

    /// Read the full inode buffer (core + data fork) for `ino`.
    fn read_inode(&self, ino: u64) -> io::Result<Vec<u8>> {
        let offset = self.inode_offset(ino);
        self.read_inode_at_offset(offset)
    }

    /// Return the size of the inode core based on the inode version.
    /// v2 inodes have a 96-byte core; v3/v5 inodes have a 176-byte core.
    /// The data fork starts immediately after the core.
    fn inode_core_size(inode: &[u8]) -> usize {
        if inode.len() > di_off::VERSION {
            let version = inode[di_off::VERSION];
            if version == 3 {
                return INODE_CORE_SIZE_V3;
            }
        }
        INODE_CORE_SIZE
    }

    /// Return the slice of the inode buffer that holds the data fork.
    ///
    /// For LOCAL-format (shortform), the data fork fills the entire literal
    /// area regardless of `di_forkoff`. For EXTENTS/BTREE formats, `di_forkoff`
    /// correctly delimits the extent/btree region from the attribute fork.
    fn data_fork(inode: &[u8]) -> io::Result<&[u8]> {
        let core_size = Self::inode_core_size(inode);
        let literal = &inode[core_size..];
        let format = inode[di_off::FORMAT];
        if format == FORMAT_LOCAL {
            // Shortform directories: entire literal area is data fork
            Ok(literal)
        } else {
            let forkoff = inode[di_off::FORKOFF] as usize;
            if forkoff == 0 {
                Ok(literal)
            } else if forkoff > literal.len() {
                Err(invalid_fs_data(format!(
                    "di_forkoff {} exceeds literal area {}",
                    forkoff,
                    literal.len()
                )))
            } else {
                Ok(&literal[..forkoff])
            }
        }
    }

    /// Maximum number of complete extent records that fit in the data fork.
    fn max_inline_extents(inode: &[u8]) -> usize {
        Self::data_fork(inode).map(|df| df.len() / BMBT_REC_SIZE).unwrap_or(0)
    }

    /// Number of extent records declared for this inode.
    fn nextents(inode: &[u8]) -> u32 {
        be_u32(inode, di_off::NEXTENTS)
    }

    /// Decode a single bmbt record’s physical start-block and block count.
    ///
    /// Encoding: l0 holds the file logical offset, l1 encodes start-block
    /// in the high 43 bits and block count in the low 21 bits.
    fn decode_extent(rec: &[u8]) -> (u64, u64, u64) {
        let l0 = be_u64(rec, 0);
        let l1 = be_u64(rec, 8);
        let logical = l0; // file block offset
        let start_block = l1 >> 21;
        let block_count = l1 & 0x1F_FFFF;
        (logical, start_block, block_count)
    }

    // ------------------------------------------------------------------
    // Data-fork parsers by di_format
    // ------------------------------------------------------------------

    /// Parse shortform-directory entries from an inode's data fork.
    ///
    /// Returns `Vec<(name, inode_number, is_dir)>`.  The caller is
    /// responsible for distinguishing files from directories via
    /// `di_mode` of the target inode, but the shortform entry itself
    /// does not carry a file-type byte in v2 (no `ftype`).  We store
    /// `is_dir = false` here and let higher-level resolution decide.
    /// Whether this inode's filesystem has the `ftype` feature (di_version >= 3).
    fn has_ftype(inode: &[u8]) -> bool {
        inode.len() > di_off::VERSION && inode[di_off::VERSION] >= 3
    }

    fn parse_shortform_dir(
        data_fork: &[u8],
        has_ftype: bool,
    ) -> io::Result<Vec<(String, u64)>> {
        // Dynamic header: parent is 4 bytes when i8count==0, 8 bytes otherwise.
        let min_hdr = DIR2_SF_HDR_4;
        if data_fork.len() < min_hdr {
            return Err(invalid_fs_data("shortform dir too small for header"));
        }
        let count = data_fork[0] as usize;
        let i8count = data_fork[1] as usize;

        // Header layout: count(1) + i8count(1) + parent(4 or 8 bytes).
        let hdr_size = if i8count == 0 { DIR2_SF_HDR_4 } else { DIR2_SF_HDR_8 };
        if data_fork.len() < hdr_size {
            return Err(invalid_fs_data("shortform dir header truncated"));
        }

        // The LAST `i8count` entries have 8-byte inode numbers.
        // The first `count - i8count` entries have 4-byte inode numbers.
        // When i8count == 0, ALL entries use 4-byte inodes.
        let first_8byte_idx = count.saturating_sub(i8count);

        let mut pos = hdr_size;
        let mut entries = Vec::with_capacity(count);
        for i in 0..count {
            if pos + 3 > data_fork.len() {
                break;
            }
            let namelen = data_fork[pos] as usize;
            let name_start = pos + 3;
            let name_end = name_start + namelen;
            let name = String::from_utf8_lossy(&data_fork[name_start..name_end.min(data_fork.len())])
                .to_string();

            let uses_8byte = i >= first_8byte_idx;
            let (inode_val, tail_len) = if has_ftype {
                // v3/v5: ftype(1) + inode(4 or 8)
                let inode_off = name_end + 1;
                if uses_8byte {
                    if inode_off + 8 > data_fork.len() { break; }
                    (be_u64(data_fork, inode_off), 9)
                } else {
                    if inode_off + 4 > data_fork.len() { break; }
                    (be_u32(data_fork, inode_off) as u64, 5)
                }
            } else {
                if uses_8byte {
                    if name_end + 8 > data_fork.len() { break; }
                    (be_u64(data_fork, name_end), 8)
                } else {
                    if name_end + 4 > data_fork.len() { break; }
                    (be_u32(data_fork, name_end) as u64, 4)
                }
            };
            entries.push((name, inode_val));
            pos = name_end + tail_len;
        }
        Ok(entries)
    }

    /// Read file data from an extent-mapped inode (di_format = 2).
    fn read_extent_data(&self, inode: &[u8], file_size: u64) -> io::Result<Vec<u8>> {
        let df = Self::data_fork(inode)?;
        // Cap at complete records fitting in data fork (forkoff may truncate)
        let max_extents = Self::max_inline_extents(inode);
        let nextents = Self::nextents(inode) as usize;
        let count = nextents.min(max_extents);
        let mut data = Vec::new();

        for i in 0..count {
            let off = i * BMBT_REC_SIZE;
            if off + BMBT_REC_SIZE > df.len() {
                break;
            }
            let (_logical, start_block, block_count) = Self::decode_extent(&df[off..]);
            for blk in 0..block_count {
                let block_data = self.read_block(start_block + blk)?;
                data.extend_from_slice(&block_data);
            }
        }

        Ok(truncate_data_to_declared_size(data, file_size))
    }

    /// Read file data from a B+tree-mapped inode (di_format = 3).
    ///
    /// The data fork contains a bmbt root block whose leaf records
    /// describe extents.  This reader handles level-0 (leaf) root
    /// blocks; deeper trees would require recursive btree walking.
    fn read_btree_data(&self, inode: &[u8], file_size: u64) -> io::Result<Vec<u8>> {
        let df = Self::data_fork(inode)?;
        if df.len() < 8 {
            return Ok(Vec::new());
        }

        let magic = be_u32(df, 0);
        if magic != BMAP_MAGIC {
            return Err(invalid_fs_data(format!(
                "invalid bmbt block magic 0x{:08X}",
                magic
            )));
        }

        let level = be_u16(df, 4);
        let numrecs = be_u16(df, 6) as usize;

        // Leaf-node header is 24 bytes; records follow.
        const BMAP_LEAF_HDR: usize = 24;

        if level != 0 {
            // For internal nodes, we would recurse.  For the synthetic
            // fixture a level-0 root is sufficient.
            return Err(invalid_fs_data(format!(
                "bmbt btree level {} not supported in this reader",
                level
            )));
        }

        let mut data = Vec::new();
        let rec_start = BMAP_LEAF_HDR;
        // Each leaf record: key (u64) + extent (2 × u64) = 24 bytes.
        const LEAF_REC_SIZE: usize = 24;

        for i in 0..numrecs {
            let off = rec_start + i * LEAF_REC_SIZE;
            if off + LEAF_REC_SIZE > df.len() {
                break;
            }
            // Skip key (8 bytes), then extent (16 bytes) at off+8.
            let (_logical, start_block, block_count) = Self::decode_extent(&df[off + 8..off + 24]);
            for blk in 0..block_count {
                let block_data = self.read_block(start_block + blk)?;
                data.extend_from_slice(&block_data);
            }
        }

        Ok(truncate_data_to_declared_size(data, file_size))
    }

    /// Read an inode's data bytes according to its `di_format`.
    fn read_file_content(&self, ino: u64) -> io::Result<Vec<u8>> {
        let inode = self.read_inode(ino)?;
        Self::validate_inode_magic(&inode)?;

        let format = inode[di_off::FORMAT];
        let size = be_u64(&inode, di_off::SIZE);

        match format {
            FORMAT_LOCAL => {
                // For files, LOCAL means data is inline in the fork.
                let df = Self::data_fork(&inode)?;
                Ok(truncate_data_to_declared_size(df.to_vec(), size))
            }
            FORMAT_EXTENTS => self.read_extent_data(&inode, size),
            FORMAT_BTREE => self.read_btree_data(&inode, size),
            other => Err(invalid_fs_data(format!("unsupported di_format {}", other))),
        }
    }

    /// Validate the inode magic (`IN`).
    fn validate_inode_magic(inode: &[u8]) -> io::Result<()> {
        if inode.len() < 2 {
            return Err(invalid_fs_data("inode buffer too short"));
        }
        let magic = be_u16(inode, di_off::MAGIC);
        if magic != XFS_INODE_MAGIC {
            return Err(invalid_fs_data(format!(
                "invalid inode magic 0x{:04X}, expected 0x{:04X}",
                magic, XFS_INODE_MAGIC
            )));
        }
        Ok(())
    }

    /// Return `true` when an inode's mode bits indicate a directory.
    fn inode_is_dir(inode: &[u8]) -> bool {
        (be_u16(inode, di_off::MODE) & S_IFDIR) != 0
    }

    /// Parse block-format directory entries from extent-backed data.
    ///
    /// The data starts with one `xfs_dir3_data_hdr` (v5, 64 bytes) or
    /// `xfs_dir2_data_hdr` (v3, 32 bytes), determined by magic.  Entries
    /// are `xfs_dir2_data_entry` records carrying inumber(8), namelen(1),
    /// name(n), ftype(1).  Free-space records tagged with 0xFFFF are
    /// skipped.  The ftype byte (2 = directory) is used directly so no
    /// child-inode read is needed.
    fn parse_block_dir(data: &[u8]) -> io::Result<Vec<(String, u64, bool)>> {
        if data.len() < 8 {
            return Ok(Vec::new());
        }
        let magic = be_u32(data, 0);
        let hdr_size = if magic == XFS_DIR3_BLOCK_MAGIC {
            XFS_DIR3_DATA_HDR_SIZE
        } else if magic == XFS_DIR2_BLOCK_MAGIC {
            XFS_DIR2_DATA_HDR_SIZE
        } else {
            return Err(invalid_fs_data(format!(
                "unknown block directory magic 0x{:08X}",
                magic
            )));
        };
        if data.len() <= hdr_size {
            return Ok(Vec::new());
        }

        // Block-format directories pack the entry region, then a
        // `xfs_dir2_leaf_entry[count]` array (8 bytes each), then an 8-byte
        // `xfs_dir2_block_tail_t` (count, stale) at the very end of the
        // block. Without this the entry loop wanders into the leaf/tail
        // region and decodes it as garbage entries.
        let data_end = if data.len() >= 8 {
            let leaf_count = be_u32(data, data.len() - 8) as usize;
            data.len()
                .saturating_sub(8)
                .saturating_sub(leaf_count * 8)
                .max(hdr_size)
        } else {
            data.len()
        };

        let mut pos = hdr_size;
        let mut entries = Vec::new();
        while pos + 16 <= data_end {
            // Check for free-space record (freetag = 0xFFFF)
            let freetag = u16::from_be_bytes([data[pos], data[pos + 1]]);
            if freetag == XFS_DIR2_FREE_TAG {
                if pos + 4 > data.len() {
                    break;
                }
                let skip_len = u16::from_be_bytes([data[pos + 2], data[pos + 3]]) as usize;
                pos = pos.saturating_add(skip_len.max(4));
                continue;
            }

            // Parse xfs_dir2_data_entry: inumber(8) + namelen(1) + name(n) + ftype(1)
            let inumber = be_u64(data, pos);
            let namelen = data[pos + 8] as usize;
            let name_start = pos + 9; // skip inumber(8) + namelen(1)
            if namelen == 0 {
                pos = pos.saturating_add(16);
                continue;
            }
            let entry_body_end = name_start + namelen;
            if entry_body_end + 1 > data.len() {
                break;
            }
            let ftype = data[entry_body_end];
            let name = String::from_utf8_lossy(&data[name_start..entry_body_end]).to_string();
            let is_dir = ftype == XFS_DIR3_FT_DIR;

            // xfs_dir2_data_entry is inumber(8) + namelen(1) + name(n) +
            // ftype(1) + tag(2), 8-byte aligned. The trailing 2-byte tag
            // (the entry's own starting offset, used for reverse lookup)
            // has no bearing on parsing but must be included when
            // advancing past the entry, or every later entry in the
            // block desyncs and decodes as garbage.
            let raw_end = entry_body_end + 1 + 2; // ftype(1) + tag(2)
            let padded_end = raw_end + ((-(raw_end as isize as i64)) & 7) as usize;
            pos = padded_end;

            if inumber != 0 {
                entries.push((name, inumber, is_dir));
            }
        }
        Ok(entries)
    }

    /// Read a directory's shortform entries from its inode and annotate
    /// each entry with `is_dir` by peeking at the child inode's mode.
    fn read_directory_entries(&self, ino: u64) -> io::Result<Vec<(String, u64, bool)>> {
        let inode = self.read_inode(ino)?;
        Self::validate_inode_magic(&inode)?;

        if !Self::inode_is_dir(&inode) {
            return Err(invalid_fs_data(format!("inode {} is not a directory", ino)));
        }

        let format = inode[di_off::FORMAT];
        match format {
            FORMAT_LOCAL => {
                // Shortform directory: entries are inline in the data fork.
                let df = Self::data_fork(&inode)?;
                let raw = Self::parse_shortform_dir(df, Self::has_ftype(&inode))?;
                let mut entries = Vec::with_capacity(raw.len());
                for (name, child_ino) in raw {
                    let is_dir = self
                        .read_inode(child_ino)
                        .ok()
                        .filter(|ci| ci.len() >= 4)
                        .is_some_and(|ci| Self::inode_is_dir(&ci));
                    entries.push((name, child_ino, is_dir));
                }
                Ok(entries)
            }
            FORMAT_EXTENTS => {
                // Block directory: entries are in extent-backed data blocks.
                // Use u64::MAX so all extent blocks are read (directory data may
                // be smaller than the declared size).
                let data = self.read_extent_data(&inode, u64::MAX)?;
                Self::parse_block_dir(&data)
            }
            FORMAT_BTREE => {
                let data = self.read_btree_data(&inode, u64::MAX)?;
                Self::parse_block_dir(&data)
            }
            other => Err(invalid_fs_data(format!(
                "directory inode {} uses unsupported format {}",
                ino, other
            ))),
        }
    }

    /// Walk a path string to resolve an inode number and whether it is
    /// a directory.  Returns `None` when a component does not exist.
    fn resolve_path(&self, path: &str) -> io::Result<Option<(u64, bool)>> {
        let components = path_components(path);
        if components.is_empty() {
            return Ok(Some((self.root_ino, true)));
        }

        let mut current_ino = self.root_ino;
        for (i, component) in components.iter().enumerate() {
            let entries = self.read_directory_entries(current_ino)?;
            let is_last = i == components.len() - 1;
            let found = entries.iter().find(|(name, _, _)| name == component);
            match found {
                Some((_, ino, is_dir)) => {
                    if is_last {
                        return Ok(Some((*ino, *is_dir)));
                    }
                    if !is_dir {
                        return Ok(None); // intermediate component is a file
                    }
                    current_ino = *ino;
                }
                None => return Ok(None),
            }
        }
        Ok(None)
    }
}

// ---------------------------------------------------------------------------
// FileSystemReader trait
// ---------------------------------------------------------------------------

impl FileSystemReader for XfsReader {
    fn root(&self) -> io::Result<FsNode> {
        Ok(root_node())
    }

    fn list_children(&self, path: &str) -> io::Result<Vec<FsNode>> {
        let (ino, is_dir) = self
            .resolve_path(path)?
            .ok_or_else(|| path_not_found(path))?;
        if !is_dir {
            return Err(path_is_not_directory(path));
        }

        let entries = self.read_directory_entries(ino)?;
        let mut nodes = Vec::new();
        for (name, _child_ino, child_is_dir) in entries {
            if is_special_directory_name(&name) {
                continue;
            }
            nodes.push(fs_node_without_timestamps(name, child_is_dir, 0));
        }
        Ok(child_nodes_with_parent_path(nodes, path))
    }

    fn open_file(&self, path: &str) -> io::Result<Box<dyn Read>> {
        let (ino, is_dir) = self
            .resolve_path(path)?
            .ok_or_else(|| file_not_found(path))?;
        if is_dir {
            return Err(path_is_directory(path));
        }
        let data = self.read_file_content(ino)?;
        Ok(Box::new(io::Cursor::new(data)))
    }

    fn data_source_name(&self) -> &str {
        "xfs"
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use evidence_core::ReaderInfo;
    use std::io::{Read, Seek};

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
        df_file[0..8].copy_from_slice(&0u64.to_be_bytes()); // l0: logical offset 0
        let l1: u64 = (4u64 << 21) | 1; // start block 4, 1 block
        df_file[8..16].copy_from_slice(&l1.to_be_bytes());

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
                                                          // Leaf records start at offset 24.
                                                          // Record: key(8 bytes, u64) + extent(16 bytes: l0 + l1)
        let rec_off: usize = 24;
        df_hi[rec_off..rec_off + 8].copy_from_slice(&0u64.to_be_bytes()); // key = file block 0
        df_hi[rec_off + 8..rec_off + 16].copy_from_slice(&0u64.to_be_bytes()); // extent l0 = 0
        let l1_val: u64 = (6u64 << 21) | 1; // start block 6, 1 block
        df_hi[rec_off + 16..rec_off + 24].copy_from_slice(&l1_val.to_be_bytes());

        // ---- Block 4: test.txt data "Hello World" ----
        img[16384..16384 + 11].copy_from_slice(b"Hello World");

        // ---- Block 6: hello.dat data "Hello subdir!" ----
        img[24576..24576 + 13].copy_from_slice(b"Hello subdir!");

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
    // block directory fixture: v3 header (32B), "XDB2" magic, EXTENTS inode
    // -----------------------------------------------------------------------

    /// Build a block-format directory buffer (v3, 32-byte header) with
    /// synthetic entries that would be stored in extent-backed data blocks.
    /// The inode itself is not part of this buffer — this is the raw data
    /// that `read_extent_data` would return.
    fn build_block_dir_v3_data() -> Vec<u8> {
        let hdr_size = XFS_DIR2_DATA_HDR_SIZE; // 32
        let block_size: usize = 512;
        let mut buf = vec![0u8; block_size];

        // v3 block header
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
        let e1_end = e1_ftype_pos + 1;
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

        buf
    }

    /// Build a block directory buffer with a free-space (0xFFFF) entry
    /// interleaved between two entries.
    fn build_block_dir_data_with_free_space() -> Vec<u8> {
        let hdr_size = XFS_DIR2_DATA_HDR_SIZE; // 32
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
        let e1_end = ft1 + 1;
        let p1 = e1_end + ((-(e1_end as isize as i64)) & 7) as usize;

        // Free-space record spanning 32 bytes
        buf[p1..p1 + 2].copy_from_slice(&XFS_DIR2_FREE_TAG.to_be_bytes());
        let free_len: u16 = 32;
        buf[p1 + 2..p1 + 4].copy_from_slice(&free_len.to_be_bytes());
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
        let e2_end = ft2 + 1;
        let _p2 = e2_end + ((-(e2_end as isize as i64)) & 7) as usize;

        buf
    }

    // -----------------------------------------------------------------------
    // test_parse_block_dir_v3
    // -----------------------------------------------------------------------

    #[test]
    fn test_parse_block_dir_v3() {
        let data = build_block_dir_v3_data();
        let entries = XfsReader::parse_block_dir(&data).unwrap();
        assert_eq!(entries.len(), 2, "v3 block should produce 2 entries");

        let file1 = entries.iter().find(|(n, _, _)| n == "file1.txt").unwrap();
        assert_eq!(file1.1, 100);
        assert!(!file1.2, "ftype=1 → not a directory");

        let sub = entries.iter().find(|(n, _, _)| n == "subdir").unwrap();
        assert_eq!(sub.1, 200);
        assert!(sub.2, "ftype=2 → is a directory");
    }

    // -----------------------------------------------------------------------
    // test_parse_block_dir_v5
    // -----------------------------------------------------------------------

    #[test]
    fn test_parse_block_dir_v5() {
        let data = build_block_dir_v5_data();
        let entries = XfsReader::parse_block_dir(&data).unwrap();
        assert_eq!(entries.len(), 1, "v5 block should produce 1 entry");
        assert_eq!(entries[0].0, "passwd");
        assert_eq!(entries[0].1, 42);
        assert!(!entries[0].2, "ftype=1 → file");
    }

    // -----------------------------------------------------------------------
    // test_parse_block_dir_with_free_space
    // -----------------------------------------------------------------------

    #[test]
    fn test_parse_block_dir_with_free_space() {
        let data = build_block_dir_data_with_free_space();
        let entries = XfsReader::parse_block_dir(&data).unwrap();
        assert_eq!(entries.len(), 2, "free-space entry should be skipped");
        assert!(entries.iter().any(|(n, _, _)| n == "good.txt"));
        assert!(entries.iter().any(|(n, _, _)| n == "keep.txt"));
    }

    // -----------------------------------------------------------------------
    // test_parse_block_dir_empty_data
    // -----------------------------------------------------------------------

    #[test]
    fn test_parse_block_dir_empty_data() {
        let entries = XfsReader::parse_block_dir(&[]).unwrap();
        assert!(entries.is_empty());
    }

    // -----------------------------------------------------------------------
    // test_parse_block_dir_unknown_magic_errors
    // -----------------------------------------------------------------------

    #[test]
    fn test_parse_block_dir_unknown_magic_errors() {
        let data = vec![0u8; 64];
        let result = XfsReader::parse_block_dir(&data);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::InvalidData);
    }
}
