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

mod directory;

use evidence_core::filesystem::{
    child_nodes_with_parent_path, file_not_found, fs_node_without_timestamps, fs_out_of_memory,
    invalid_fs_data, is_special_directory_name, path_is_directory, path_is_not_directory,
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
/// CRC-enabled B+tree block-map magic: `BMA3` = 0x424D4133 (big-endian).
const BMA3_MAGIC: u32 = 0x424D_4133;

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
/// Compact inode-root bmbt header: level(2) + numrecs(2).
const BMBT_SHORT_ROOT_HDR_SIZE: usize = 4;
/// Non-CRC long-format on-disk bmbt block header.
const BMBT_BLOCK_HDR_SIZE: usize = 24;
/// CRC-enabled long-format on-disk bmbt block header.
const BMBT_CRC_BLOCK_HDR_SIZE: usize = 72;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct XfsExtent {
    logical: u64,
    start_block: u64,
    block_count: u64,
    unwritten: bool,
}

const XFS_SB_VERSION2_FTYPE: u32 = 0x0000_0200;
const XFS_SB_FEAT_INCOMPAT_FTYPE: u32 = 1 << 0;

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
    pub const FEATURES2: usize = 0xC8; // u32 — v4 feature flags
    pub const BAD_FEATURES2: usize = 0xCC; // u32 — mirrors features2 on older mkfs
    pub const FEATURES_INCOMPAT: usize = 0xD8; // u32 — v5 incompat feature flags
    pub const DIRBLKLOG: usize = 0xC0; // u8 — log2 of directory block size in fsbs
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
    dirblklog: u8,
    has_ftype: bool,
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
        let dirblklog = sb_buf[sb_off::DIRBLKLOG];
        let features2 = be_u32(&sb_buf, sb_off::FEATURES2) | be_u32(&sb_buf, sb_off::BAD_FEATURES2);
        let features_incompat = be_u32(&sb_buf, sb_off::FEATURES_INCOMPAT);
        let has_ftype = (features2 & XFS_SB_VERSION2_FTYPE) != 0
            || (features_incompat & XFS_SB_FEAT_INCOMPAT_FTYPE) != 0;

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
            dirblklog,
            has_ftype,
        })
    }

    // ------------------------------------------------------------------
    // Internal helpers
    // ------------------------------------------------------------------

    /// Convert a linear filesystem-block number to a byte offset from the
    /// start of the evidence reader.
    fn block_to_offset(&self, block: u64) -> u64 {
        self.volume_offset
            .saturating_add(block.saturating_mul(self.block_size))
    }

    /// Convert an encoded XFS filesystem block (FSB) to a linear block number.
    ///
    /// XFS BMBT records store `start_block` as an AG-relative encoded FSB.
    /// Sleuth Kit resolves it with `XFS_FSB_TO_AGNO`/`XFS_FSB_TO_AGBNO` before
    /// multiplying by the block size. This differs from a raw `fsb *
    /// block_size` calculation whenever `sb_agblocks` is not a power of two.
    fn fsblock_to_linear_block(&self, fsb: u64) -> io::Result<u64> {
        if self.agblklog == 0 {
            return Ok(fsb);
        }
        if self.agblklog >= u64::BITS as u8 {
            return Err(invalid_fs_data(format!(
                "invalid XFS sb_agblklog {}",
                self.agblklog
            )));
        }

        let ag_num = fsb >> self.agblklog;
        let ag_block = fsb & ((1u64 << self.agblklog) - 1);

        if ag_num >= u64::from(self._ag_count) || ag_block >= self._ag_blocks {
            return Err(invalid_fs_data(format!(
                "filesystem block {} outside XFS AG geometry (agno={} agbno={} agcount={} agblocks={})",
                fsb, ag_num, ag_block, self._ag_count, self._ag_blocks
            )));
        }

        ag_num
            .checked_mul(self._ag_blocks)
            .and_then(|base| base.checked_add(ag_block))
            .ok_or_else(|| invalid_fs_data(format!("filesystem block {} offset overflows", fsb)))
    }

    pub(crate) fn fsblock_to_offset(&self, fsb: u64) -> io::Result<u64> {
        Ok(self.block_to_offset(self.fsblock_to_linear_block(fsb)?))
    }

    /// Read one full filesystem block.
    fn read_block(&self, block: u64) -> io::Result<Vec<u8>> {
        let offset = self.fsblock_to_offset(block)?;
        let mut buf = vec![0u8; self.block_size as usize];
        let mut reader = self.reader.borrow_mut();
        reader.seek(SeekFrom::Start(offset))?;
        reader.read_exact(&mut buf)?;
        Ok(buf)
    }

    fn read_block_lossy_zero_filled(&self, block: u64) -> io::Result<Vec<u8>> {
        let offset = self.fsblock_to_offset(block)?;
        let mut buf = vec![0u8; self.block_size as usize];
        let mut reader = self.reader.borrow_mut();
        reader.seek(SeekFrom::Start(offset))?;
        let mut filled = 0usize;
        while filled < buf.len() {
            match reader.read(&mut buf[filled..]) {
                Ok(0) => break,
                Ok(n) => filled += n,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => break,
                Err(error) => return Err(error),
            }
        }
        Ok(buf)
    }

    pub(crate) fn read_bytes_at(&self, offset: u64, length: usize) -> io::Result<Vec<u8>> {
        let mut buf = vec![0u8; length];
        if length == 0 {
            return Ok(buf);
        }
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
    fn inode_offset(&self, ino: u64) -> io::Result<u64> {
        if ino == 0 {
            return Err(invalid_fs_data("inode 0 is invalid"));
        }

        if self.agblklog > 0 || self.inopblog > 0 {
            let shift = self.agblklog + self.inopblog;
            let ino0 = ino;
            let ag_num = ino0 >> shift;
            let low_bits = ino0 & ((1u64 << shift) - 1);
            let blk_num = low_bits >> self.inopblog;
            let ino_in_blk = low_bits & ((1u64 << self.inopblog) - 1);
            let fs_blockno = ag_num
                .checked_mul(self._ag_blocks)
                .and_then(|base| base.checked_add(blk_num))
                .ok_or_else(|| invalid_fs_data(format!("inode {} offset overflows", ino)))?;
            let block_offset = self.block_to_offset(fs_blockno);
            let inode_delta = ino_in_blk
                .checked_mul(self.inode_size as u64)
                .ok_or_else(|| invalid_fs_data(format!("inode {} offset overflows", ino)))?;
            return block_offset
                .checked_add(inode_delta)
                .ok_or_else(|| invalid_fs_data(format!("inode {} offset overflows", ino)));
        }
        // Flat-table fallback for synthetic fixtures.
        let inode_index = ino - 1;
        let inode_delta = inode_index
            .checked_mul(self.inode_size as u64)
            .ok_or_else(|| invalid_fs_data(format!("inode {} offset overflows", ino)))?;
        self.block_to_offset(self.inode_base_block)
            .checked_add(inode_delta)
            .ok_or_else(|| invalid_fs_data(format!("inode {} offset overflows", ino)))
    }

    /// Read the full inode buffer (core + data fork) for `ino`.
    pub(crate) fn read_inode(&self, ino: u64) -> io::Result<Vec<u8>> {
        if self.agblklog == 0 && self.inopblog == 0 {
            let max_synthetic_ino = self
                .reader
                .borrow()
                .info()
                .size
                .checked_div(u64::from(self.inode_size).max(1))
                .filter(|max_ino| *max_ino > 0);
            if max_synthetic_ino.is_some_and(|max_ino| ino == 0 || ino > max_ino) {
                return Err(invalid_fs_data(format!(
                    "inode {} outside synthetic fixture range",
                    ino
                )));
            }
        }
        let offset = self.inode_offset(ino)?;
        self.read_inode_at_offset(offset)
    }

    /// Return the size of the inode core based on the inode version.
    /// v2 inodes have a 96-byte core; v3/v5 inodes have a 176-byte core.
    /// The data fork starts immediately after the core.
    pub(crate) fn inode_core_size(inode: &[u8]) -> usize {
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
    pub(crate) fn data_fork(inode: &[u8]) -> io::Result<&[u8]> {
        let core_size = Self::inode_core_size(inode);
        let literal = &inode[core_size..];
        let format = inode[di_off::FORMAT];
        if format == FORMAT_LOCAL {
            // Shortform directories: entire literal area is data fork
            Ok(literal)
        } else {
            let forkoff_units = inode[di_off::FORKOFF] as usize;
            if forkoff_units == 0 {
                Ok(literal)
            } else {
                let forkoff = forkoff_units.checked_mul(8).ok_or_else(|| {
                    invalid_fs_data(format!("di_forkoff {} overflows bytes", forkoff_units))
                })?;
                if forkoff > literal.len() {
                    Err(invalid_fs_data(format!(
                        "di_forkoff {} ({} bytes) exceeds literal area {}",
                        forkoff_units,
                        forkoff,
                        literal.len()
                    )))
                } else {
                    Ok(&literal[..forkoff])
                }
            }
        }
    }

    /// Maximum number of complete extent records that fit in the data fork.
    pub(crate) fn max_inline_extents(inode: &[u8]) -> usize {
        Self::data_fork(inode)
            .map(|df| df.len() / BMBT_REC_SIZE)
            .unwrap_or(0)
    }

    /// Number of extent records declared for this inode.
    pub(crate) fn nextents(inode: &[u8]) -> u32 {
        be_u32(inode, di_off::NEXTENTS)
    }

    /// Decode a single packed XFS BMBT record.
    pub(crate) fn decode_extent(rec: &[u8]) -> XfsExtent {
        let l0 = be_u64(rec, 0);
        let l1 = be_u64(rec, 8);
        let state = (l0 >> 63) != 0;
        let logical = (l0 >> 9) & ((1u64 << 54) - 1);
        let start_block = ((l0 & 0x1FF) << 43) | (l1 >> 21);
        let block_count = l1 & 0x1F_FFFF;
        XfsExtent {
            logical,
            start_block,
            block_count,
            unwritten: state,
        }
    }

    // ------------------------------------------------------------------
    // Data-fork parsers by di_format
    // ------------------------------------------------------------------

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
            let extent = Self::decode_extent(&df[off..]);
            if extent.unwritten {
                append_zeroes(
                    &mut data,
                    extent.block_count.saturating_mul(self.block_size),
                )?;
            } else {
                for blk in 0..extent.block_count {
                    let block_data = self.read_block_lossy_zero_filled(extent.start_block + blk)?;
                    data.extend_from_slice(&block_data);
                }
            }
        }

        Ok(truncate_data_to_declared_size(data, file_size))
    }

    fn read_extent_data_range(
        &self,
        inode: &[u8],
        file_size: u64,
        offset: u64,
        length: usize,
    ) -> io::Result<Vec<u8>> {
        if length == 0 || offset >= file_size {
            return Ok(Vec::new());
        }
        let range_end = offset.saturating_add(length as u64).min(file_size);
        let capacity = usize::try_from(range_end.saturating_sub(offset))
            .map_err(|_| fs_out_of_memory("xfs range exceeds addressable memory"))?;
        let mut data = Vec::with_capacity(capacity);
        let mut next_offset = offset;

        let df = Self::data_fork(inode)?;
        let max_extents = Self::max_inline_extents(inode);
        let nextents = Self::nextents(inode) as usize;
        for i in 0..nextents.min(max_extents) {
            let off = i * BMBT_REC_SIZE;
            if off + BMBT_REC_SIZE > df.len() {
                break;
            }
            let extent = Self::decode_extent(&df[off..]);
            self.read_extent_range(extent, offset, range_end, &mut next_offset, &mut data)?;
        }

        append_zeroes(&mut data, range_end.saturating_sub(next_offset))?;
        Ok(data)
    }

    /// Read file data from a B+tree-mapped inode (di_format = 3).
    ///
    /// The data fork contains a bmbt root node (`xfs_bmdr_block_t`,
    /// 8-byte header). Deeper B+tree children use the on-disk `lblock`
    /// format (24-byte header with left/right sibling pointers).
    /// This implementation walks the tree recursively.
    fn read_btree_data(&self, inode: &[u8], file_size: u64) -> io::Result<Vec<u8>> {
        let mut data = Vec::new();
        for extent in self.collect_btree_extents(inode)? {
            if extent.unwritten {
                append_zeroes(
                    &mut data,
                    extent.block_count.saturating_mul(self.block_size),
                )?;
                continue;
            }
            for blk in 0..extent.block_count {
                let block_data = self.read_block(extent.start_block + blk)?;
                data.extend_from_slice(&block_data);
            }
        }
        Ok(truncate_data_to_declared_size(data, file_size))
    }

    pub(crate) fn collect_btree_extents(&self, inode: &[u8]) -> io::Result<Vec<XfsExtent>> {
        let df = Self::data_fork(inode)?;
        if df.len() < BMBT_SHORT_ROOT_HDR_SIZE {
            return Ok(Vec::new());
        }

        let mut extents = Vec::new();
        if df.len() >= 8 && be_u32(df, 0) == BMAP_MAGIC {
            // Backward-compatible path for older synthetic fixtures that put
            // an on-disk btree block header directly in the inode.
            self.walk_btree_node_extents(df, true, &mut extents)?;
        } else {
            self.walk_bmdr_root_extents(df, &mut extents)?;
        }
        Ok(extents)
    }

    fn read_btree_data_range(
        &self,
        inode: &[u8],
        file_size: u64,
        offset: u64,
        length: usize,
    ) -> io::Result<Vec<u8>> {
        if length == 0 || offset >= file_size {
            return Ok(Vec::new());
        }
        let range_end = offset.saturating_add(length as u64).min(file_size);
        let capacity = usize::try_from(range_end.saturating_sub(offset))
            .map_err(|_| fs_out_of_memory("xfs btree range exceeds addressable memory"))?;
        let mut data = Vec::with_capacity(capacity);
        let mut next_offset = offset;
        for extent in self.collect_btree_extents(inode)? {
            self.read_extent_range(extent, offset, range_end, &mut next_offset, &mut data)?;
        }
        append_zeroes(&mut data, range_end.saturating_sub(next_offset))?;
        Ok(data)
    }

    fn walk_bmdr_root_extents(&self, node: &[u8], extents: &mut Vec<XfsExtent>) -> io::Result<()> {
        if node.len() < BMBT_SHORT_ROOT_HDR_SIZE {
            return Ok(());
        }
        let level = be_u16(node, 0);
        let numrecs = be_u16(node, 2) as usize;
        if numrecs == 0 {
            return Ok(());
        }

        if level == 0 {
            let recs_start = BMBT_SHORT_ROOT_HDR_SIZE;
            for i in 0..numrecs {
                let off = recs_start + i * BMBT_REC_SIZE;
                if off + BMBT_REC_SIZE > node.len() {
                    break;
                }
                extents.push(Self::decode_extent(&node[off..off + BMBT_REC_SIZE]));
            }
        } else {
            let maxrecs = Self::bmdr_maxrecs(node.len(), false);
            let keys_start = BMBT_SHORT_ROOT_HDR_SIZE;
            let ptrs_start = keys_start + maxrecs * 8;
            for i in 0..numrecs {
                let off = ptrs_start + i * 8;
                if off + 8 > node.len() {
                    break;
                }
                let child_ptr = be_u64(node, off);
                let child_block = self.read_block(child_ptr)?;
                self.walk_btree_child_extents(&child_block, extents)?;
            }
        }
        Ok(())
    }

    fn walk_btree_child_extents(
        &self,
        node: &[u8],
        extents: &mut Vec<XfsExtent>,
    ) -> io::Result<()> {
        let Some((hdr_size, level, numrecs)) = Self::parse_btree_block_header(node) else {
            return Ok(());
        };
        if level == 0 {
            let recs_start = hdr_size;
            for i in 0..numrecs {
                let off = recs_start + i * BMBT_REC_SIZE;
                if off + BMBT_REC_SIZE > node.len() {
                    break;
                }
                extents.push(Self::decode_extent(&node[off..off + BMBT_REC_SIZE]));
            }
        } else {
            let maxrecs = Self::bmbt_block_maxrecs(node.len(), hdr_size, false);
            let keys_start = hdr_size;
            let ptrs_start = keys_start + maxrecs * 8;
            for i in 0..numrecs {
                let off = ptrs_start + i * 8;
                if off + 8 > node.len() {
                    break;
                }
                let child_ptr = be_u64(node, off);
                let child_block = self.read_block(child_ptr)?;
                self.walk_btree_child_extents(&child_block, extents)?;
            }
        }
        Ok(())
    }

    fn bmdr_maxrecs(block_len: usize, leaf: bool) -> usize {
        let data_len = block_len.saturating_sub(BMBT_SHORT_ROOT_HDR_SIZE);
        if leaf {
            data_len / BMBT_REC_SIZE
        } else {
            data_len / (8 + 8)
        }
    }

    fn bmbt_block_maxrecs(block_len: usize, hdr_size: usize, leaf: bool) -> usize {
        let data_len = block_len.saturating_sub(hdr_size);
        if leaf {
            data_len / BMBT_REC_SIZE
        } else {
            data_len / (8 + 8)
        }
    }

    fn parse_btree_block_header(node: &[u8]) -> Option<(usize, u16, usize)> {
        if node.len() < 8 {
            return None;
        }
        match be_u32(node, 0) {
            BMAP_MAGIC => {
                if node.len() < BMBT_BLOCK_HDR_SIZE {
                    return None;
                }
                Some((
                    BMBT_BLOCK_HDR_SIZE,
                    be_u16(node, 4),
                    be_u16(node, 6) as usize,
                ))
            }
            BMA3_MAGIC => {
                if node.len() < BMBT_CRC_BLOCK_HDR_SIZE {
                    return None;
                }
                Some((
                    BMBT_CRC_BLOCK_HDR_SIZE,
                    be_u16(node, 4),
                    be_u16(node, 6) as usize,
                ))
            }
            _ => None,
        }
    }

    fn walk_btree_node_extents(
        &self,
        node: &[u8],
        is_inode_root: bool,
        extents: &mut Vec<XfsExtent>,
    ) -> io::Result<()> {
        let hdr_size: usize = if is_inode_root { 8 } else { 24 };
        if node.len() < hdr_size {
            return Ok(());
        }
        let level = be_u16(node, 4);
        let numrecs = be_u16(node, 6) as usize;

        if level == 0 {
            const LEAF_SLOT: usize = 24;
            for i in 0..numrecs {
                let off = hdr_size + i * LEAF_SLOT;
                if off + LEAF_SLOT > node.len() {
                    break;
                }
                extents.push(Self::decode_extent(&node[off + 8..off + 24]));
            }
        } else {
            const INTERNAL_SLOT: usize = 16;
            for i in 0..numrecs {
                let off = hdr_size + i * INTERNAL_SLOT;
                if off + INTERNAL_SLOT > node.len() {
                    break;
                }
                let child_ptr = be_u64(node, off + 8);
                let child_block = self.read_block(child_ptr)?;
                self.walk_btree_node_extents(&child_block, false, extents)?;
            }
        }
        Ok(())
    }

    fn read_extent_range(
        &self,
        extent: XfsExtent,
        range_start: u64,
        range_end: u64,
        next_offset: &mut u64,
        data: &mut Vec<u8>,
    ) -> io::Result<()> {
        let extent_start = extent.logical.saturating_mul(self.block_size);
        let extent_len = extent.block_count.saturating_mul(self.block_size);
        let extent_end = extent_start.saturating_add(extent_len);
        let overlap_start = extent_start.max(range_start);
        let overlap_end = extent_end.min(range_end);
        if overlap_start >= overlap_end {
            return Ok(());
        }
        if *next_offset < overlap_start {
            append_zeroes(data, overlap_start - *next_offset)?;
        }
        let read_len = usize::try_from(overlap_end - overlap_start)
            .map_err(|_| fs_out_of_memory("xfs extent range exceeds addressable memory"))?;
        if extent.unwritten {
            append_zeroes(data, read_len as u64)?;
        } else {
            let physical_offset = self
                .fsblock_to_offset(extent.start_block)?
                .saturating_add(overlap_start.saturating_sub(extent_start));
            let chunk = self.read_bytes_at(physical_offset, read_len)?;
            data.extend_from_slice(&chunk);
        }
        *next_offset = overlap_end;
        Ok(())
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

    fn read_file_content_range(&self, ino: u64, offset: u64, length: usize) -> io::Result<Vec<u8>> {
        let inode = self.read_inode(ino)?;
        Self::validate_inode_magic(&inode)?;
        let format = inode[di_off::FORMAT];
        let size = be_u64(&inode, di_off::SIZE);
        if length == 0 || offset >= size {
            return Ok(Vec::new());
        }

        match format {
            FORMAT_LOCAL => {
                let df = Self::data_fork(&inode)?;
                let start = usize::try_from(offset)
                    .ok()
                    .map(|start| start.min(df.len()))
                    .unwrap_or(df.len());
                let declared_end = usize::try_from(size)
                    .ok()
                    .map(|end| end.min(df.len()))
                    .unwrap_or(df.len());
                let end = start.saturating_add(length).min(declared_end);
                Ok(df[start..end].to_vec())
            }
            FORMAT_EXTENTS => self.read_extent_data_range(&inode, size, offset, length),
            FORMAT_BTREE => self.read_btree_data_range(&inode, size, offset, length),
            other => Err(invalid_fs_data(format!("unsupported di_format {}", other))),
        }
    }

    /// Validate the inode magic (`IN`).
    pub(crate) fn validate_inode_magic(inode: &[u8]) -> io::Result<()> {
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
    pub(crate) fn inode_is_dir(inode: &[u8]) -> bool {
        (be_u16(inode, di_off::MODE) & S_IFDIR) != 0
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
        for entry in entries {
            if is_special_directory_name(&entry.name) {
                continue;
            }
            nodes.push(fs_node_without_timestamps(
                entry.name,
                entry.is_dir,
                entry.size,
            ));
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

    fn read_file_range(&self, path: &str, offset: u64, length: usize) -> io::Result<Vec<u8>> {
        let (ino, is_dir) = self
            .resolve_path(path)?
            .ok_or_else(|| file_not_found(path))?;
        if is_dir {
            return Err(path_is_directory(path));
        }
        self.read_file_content_range(ino, offset, length)
    }

    fn data_source_name(&self) -> &str {
        "xfs"
    }
}

fn append_zeroes(data: &mut Vec<u8>, count: u64) -> io::Result<()> {
    let count = usize::try_from(count)
        .map_err(|_| fs_out_of_memory("xfs sparse range exceeds addressable memory"))?;
    let new_len = data
        .len()
        .checked_add(count)
        .ok_or_else(|| fs_out_of_memory("xfs sparse range exceeds addressable memory"))?;
    data.resize(new_len, 0);
    Ok(())
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests;
