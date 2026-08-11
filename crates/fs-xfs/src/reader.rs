use crate::directory::BoundedDirectoryEntryCache;
use crate::inode_cache::BoundedInodeBlockCache;
use crate::log::XfsLogGeometry;
use evidence_core::filesystem::{
    invalid_fs_data, unsupported_fs, FileSystemDiagnostic, FileSystemReadMetrics,
};
use evidence_core::EvidenceReader;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::io::{self, Read, Seek, SeekFrom};
use std::sync::Arc;

pub(crate) const XFS_SUPER_MAGIC: u32 = 0x5846_5342;
pub(crate) const XFS_INODE_MAGIC: u16 = 0x494E;
pub(crate) const BMAP_MAGIC: u32 = 0x424D_4150;
pub(crate) const BMA3_MAGIC: u32 = 0x424D_4133;
pub(crate) const S_IFMT: u16 = 0xF000;
pub(crate) const S_IFDIR: u16 = 0x4000;
pub(crate) const FORMAT_LOCAL: u8 = 1;
pub(crate) const FORMAT_EXTENTS: u8 = 2;
pub(crate) const FORMAT_BTREE: u8 = 3;

pub(crate) const INODE_CORE_SIZE: usize = 100;
pub(crate) const INODE_CORE_SIZE_V3: usize = 176;
pub(crate) const BMBT_REC_SIZE: usize = 16;
pub(crate) const BMBT_SHORT_ROOT_HDR_SIZE: usize = 4;
pub(crate) const BMBT_BLOCK_HDR_SIZE: usize = 24;
pub(crate) const BMBT_CRC_BLOCK_HDR_SIZE: usize = 72;
const XFS_SB_VERSION2_FTYPE: u32 = 0x0000_0200;
pub(crate) const XFS_SB_FEAT_INCOMPAT_FTYPE: u32 = 1 << 0;
pub(crate) const XFS_SB_FEAT_INCOMPAT_SPINODES: u32 = 1 << 1;
pub(crate) const XFS_SB_FEAT_INCOMPAT_META_UUID: u32 = 1 << 2;
pub(crate) const XFS_SB_FEAT_INCOMPAT_BIGTIME: u32 = 1 << 3;
pub(crate) const XFS_SB_FEAT_INCOMPAT_NEEDSREPAIR: u32 = 1 << 4;
pub(crate) const XFS_SB_FEAT_INCOMPAT_NREXT64: u32 = 1 << 5;
/// Incompatible-feature bits whose on-disk dinode/directory layout this
/// parser understands. Anything outside this mask may change the layout, so
/// the filesystem is rejected instead of being misparsed.
pub(crate) const XFS_SB_FEAT_INCOMPAT_SUPPORTED: u32 = XFS_SB_FEAT_INCOMPAT_FTYPE
    | XFS_SB_FEAT_INCOMPAT_SPINODES
    | XFS_SB_FEAT_INCOMPAT_META_UUID
    | XFS_SB_FEAT_INCOMPAT_BIGTIME
    | XFS_SB_FEAT_INCOMPAT_NEEDSREPAIR
    | XFS_SB_FEAT_INCOMPAT_NREXT64;
const XFS_SB_VERSION_NUMBITS: u16 = 0x000F;
const XFS_SB_VERSION_5: u16 = 5;
const MIN_BLOCK_SIZE: u64 = 512;
const MAX_BLOCK_SIZE: u64 = 65536;
/// Cap on a fully buffered file read, mirroring fs-ntfs.
pub(crate) const MAX_BUFFERED_FILE_BYTES: u64 = 256 * 1024 * 1024;
/// Cap on the total bytes a single directory enumeration may read.
pub(crate) const MAX_DIRECTORY_SCAN_BYTES: u64 = 256 * 1024 * 1024;
/// Deepest bmbt root level accepted; each child must sit exactly one level
/// below its parent, so this also bounds the recursion depth.
pub(crate) const MAX_BMBT_TREE_DEPTH: u16 = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct XfsExtent {
    pub(crate) logical: u64,
    pub(crate) start_block: u64,
    pub(crate) block_count: u64,
    pub(crate) unwritten: bool,
}

pub(crate) mod sb_off {
    pub const MAGIC: usize = 0x00;
    pub const BLOCKSIZE: usize = 0x04;
    pub const DBLOCKS: usize = 0x08;
    pub const UUID: usize = 0x20;
    pub const LOGSTART: usize = 0x30;
    pub const ROOTINO: usize = 0x38;
    pub const AGBLOCKS: usize = 0x54;
    pub const AGCOUNT: usize = 0x58;
    pub const LOGBLOCKS: usize = 0x60;
    pub const VERSIONNUM: usize = 0x64;
    pub const SECTSIZE: usize = 0x66;
    pub const INODESIZE: usize = 0x68;
    pub const INOPBLOCK: usize = 0x6A;
    pub const INOPBLOG: usize = 0x7B;
    pub const AGBLKLOG: usize = 0x7C;
    pub const FEATURES2: usize = 0xC8;
    pub const BAD_FEATURES2: usize = 0xCC;
    pub const FEATURES_INCOMPAT: usize = 0xD8;
    pub const DIRBLKLOG: usize = 0xC0;
    pub const LOGSECTSIZE: usize = 0xC2;
}

pub(crate) mod di_off {
    pub const MAGIC: usize = 0x00;
    pub const MODE: usize = 0x02;
    pub const VERSION: usize = 0x04;
    pub const FORMAT: usize = 0x05;
    pub const ATIME: usize = 0x20;
    pub const MTIME: usize = 0x28;
    pub const CTIME: usize = 0x30;
    pub const SIZE: usize = 0x38;
    pub const NEXTENTS: usize = 0x4C;
    pub const FORKOFF: usize = 0x52;
    pub const FLAGS2: usize = 0x78;
    pub const CRTIME: usize = 0x90;
}

pub(crate) fn be_u16(buf: &[u8], off: usize) -> u16 {
    u16::from_be_bytes([buf[off], buf[off + 1]])
}

pub(crate) fn be_u32(buf: &[u8], off: usize) -> u32 {
    u32::from_be_bytes([buf[off], buf[off + 1], buf[off + 2], buf[off + 3]])
}

pub(crate) fn be_u64(buf: &[u8], off: usize) -> u64 {
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

pub struct XfsReader {
    pub(crate) reader: RefCell<Box<dyn EvidenceReader>>,
    pub(crate) directory_path_cache: RefCell<HashMap<String, u64>>,
    pub(crate) file_path_cache: RefCell<HashMap<String, u64>>,
    pub(crate) unverified_file_path_cache: RefCell<HashSet<String>>,
    pub(crate) directory_inode_cache: RefCell<HashMap<u64, Vec<u8>>>,
    pub(crate) directory_entry_cache: RefCell<BoundedDirectoryEntryCache>,
    pub(crate) inode_block_cache: RefCell<BoundedInodeBlockCache>,
    pub(crate) read_metrics: RefCell<FileSystemReadMetrics>,
    pub(crate) diagnostics: RefCell<Vec<FileSystemDiagnostic>>,
    pub(crate) block_size: u64,
    pub(crate) dblocks: u64,
    pub(crate) _ag_blocks: u64,
    pub(crate) _ag_count: u32,
    pub(crate) inode_size: u16,
    pub(crate) inode_cache_page_size: u64,
    pub(crate) _inopblock: u16,
    pub(crate) root_ino: u64,
    pub(crate) volume_offset: u64,
    pub(crate) inode_base_block: u64,
    pub(crate) agblklog: u8,
    pub(crate) inopblog: u8,
    pub(crate) dirblklog: u8,
    pub(crate) has_ftype: bool,
    pub(crate) log_geometry: XfsLogGeometry,
}

impl XfsReader {
    pub fn open(mut reader: Box<dyn EvidenceReader>, offset: u64) -> io::Result<Self> {
        reader.seek(SeekFrom::Start(offset))?;
        let mut sb_buf = [0u8; 512];
        reader.read_exact(&mut sb_buf)?;

        let magic = be_u32(&sb_buf, sb_off::MAGIC);
        if magic != XFS_SUPER_MAGIC {
            return Err(invalid_fs_data(format!(
                "not a valid XFS filesystem (magic 0x{magic:08X})"
            )));
        }

        let block_size = u64::from(be_u32(&sb_buf, sb_off::BLOCKSIZE));
        let dblocks = be_u64(&sb_buf, sb_off::DBLOCKS);
        let ag_count = be_u32(&sb_buf, sb_off::AGCOUNT);
        let ag_blocks_from_sb = u64::from(be_u32(&sb_buf, sb_off::AGBLOCKS));
        let inode_size = be_u16(&sb_buf, sb_off::INODESIZE);
        let inopblock = be_u16(&sb_buf, sb_off::INOPBLOCK);
        let root_ino = be_u64(&sb_buf, sb_off::ROOTINO);

        if !(MIN_BLOCK_SIZE..=MAX_BLOCK_SIZE).contains(&block_size) || !block_size.is_power_of_two()
        {
            return Err(invalid_fs_data(format!(
                "invalid XFS block size {block_size} (need a power of two in {MIN_BLOCK_SIZE}..={MAX_BLOCK_SIZE})"
            )));
        }
        if dblocks == 0 || ag_count == 0 {
            return Err(invalid_fs_data("invalid XFS superblock geometry"));
        }
        if !inode_size.is_power_of_two()
            || inode_size < INODE_CORE_SIZE_V3 as u16
            || u64::from(inode_size) > block_size
        {
            return Err(invalid_fs_data(format!(
                "invalid XFS inode size {inode_size} (need a power of two in {INODE_CORE_SIZE_V3}..={block_size})"
            )));
        }
        let ag_blocks = if ag_blocks_from_sb != 0 {
            ag_blocks_from_sb
        } else {
            dblocks / u64::from(ag_count)
        };
        let agblklog = sb_buf[sb_off::AGBLKLOG];
        let inopblog = sb_buf[sb_off::INOPBLOG];
        let dirblklog = sb_buf[sb_off::DIRBLKLOG];
        let features2 = be_u32(&sb_buf, sb_off::FEATURES2) | be_u32(&sb_buf, sb_off::BAD_FEATURES2);
        let features_incompat = be_u32(&sb_buf, sb_off::FEATURES_INCOMPAT);
        let version_major = be_u16(&sb_buf, sb_off::VERSIONNUM) & XFS_SB_VERSION_NUMBITS;
        if version_major == XFS_SB_VERSION_5 {
            let unknown = features_incompat & !XFS_SB_FEAT_INCOMPAT_SUPPORTED;
            if unknown != 0 {
                return Err(unsupported_fs(format!(
                    "XFS v5 superblock sets unknown sb_features_incompat bits 0x{unknown:08X}"
                )));
            }
        }
        let has_ftype = (features2 & XFS_SB_VERSION2_FTYPE) != 0
            || (features_incompat & XFS_SB_FEAT_INCOMPAT_FTYPE) != 0;
        let log_geometry = XfsLogGeometry::from_superblock(&sb_buf);
        let inode_cache_page_size =
            aligned_inode_cache_page_size(block_size, reader.preferred_read_granularity());
        let mut directory_path_cache = HashMap::new();
        directory_path_cache.insert(String::new(), root_ino);

        Ok(Self {
            reader: RefCell::new(reader),
            directory_path_cache: RefCell::new(directory_path_cache),
            file_path_cache: RefCell::new(HashMap::new()),
            unverified_file_path_cache: RefCell::new(HashSet::new()),
            directory_inode_cache: RefCell::new(HashMap::new()),
            directory_entry_cache: RefCell::new(BoundedDirectoryEntryCache::default()),
            inode_block_cache: RefCell::new(BoundedInodeBlockCache::default()),
            read_metrics: RefCell::new(FileSystemReadMetrics::default()),
            diagnostics: RefCell::new(Vec::new()),
            block_size,
            dblocks,
            _ag_blocks: ag_blocks,
            _ag_count: ag_count,
            inode_size,
            inode_cache_page_size,
            _inopblock: inopblock,
            root_ino,
            volume_offset: offset,
            inode_base_block: 2,
            agblklog,
            inopblog,
            dirblklog,
            has_ftype,
            log_geometry,
        })
    }

    fn read_inode_at_offset(&self, offset: u64) -> io::Result<Vec<u8>> {
        let inode_size = usize::from(self.inode_size);
        let Some((block_offset, inode_offset, page_length)) =
            self.inode_cache_range(offset, inode_size)
        else {
            self.read_metrics.borrow_mut().metadata_cache_misses += 1;
            return self.read_bytes_at(offset, inode_size);
        };
        if let Some(block) = self.inode_block_cache.borrow_mut().get(block_offset) {
            self.read_metrics.borrow_mut().metadata_cache_hits += 1;
            return Ok(block[inode_offset..inode_offset + inode_size].to_vec());
        }

        self.read_metrics.borrow_mut().metadata_cache_misses += 1;
        let block: Arc<[u8]> = Arc::from(self.read_bytes_at(block_offset, page_length)?);
        let inode = block[inode_offset..inode_offset + inode_size].to_vec();
        self.inode_block_cache
            .borrow_mut()
            .insert(block_offset, block);
        Ok(inode)
    }

    fn inode_cache_range(&self, offset: u64, inode_size: usize) -> Option<(u64, usize, usize)> {
        let page_size = usize::try_from(self.inode_cache_page_size).ok()?;
        if page_size == 0 || inode_size > page_size || offset < self.volume_offset {
            return None;
        }
        let relative = offset.checked_sub(self.volume_offset)?;
        let block_start = relative
            .checked_div(self.inode_cache_page_size)?
            .checked_mul(self.inode_cache_page_size)?;
        let block_offset = self.volume_offset.checked_add(block_start)?;
        let inode_offset = usize::try_from(offset.checked_sub(block_offset)?).ok()?;
        let filesystem_length = self.dblocks.checked_mul(self.block_size)?;
        let remaining = usize::try_from(filesystem_length.checked_sub(block_start)?).ok()?;
        let page_length = page_size.min(remaining);
        (inode_offset.checked_add(inode_size)? <= page_length).then_some((
            block_offset,
            inode_offset,
            page_length,
        ))
    }

    fn inode_offset(&self, ino: u64) -> io::Result<u64> {
        if ino == 0 {
            return Err(invalid_fs_data("inode 0 is invalid"));
        }

        if self.agblklog > 0 || self.inopblog > 0 {
            let shift = self.agblklog.checked_add(self.inopblog).ok_or_else(|| {
                invalid_fs_data(format!(
                    "invalid XFS inode geometry agblklog={} inopblog={}",
                    self.agblklog, self.inopblog
                ))
            })?;
            if shift >= u64::BITS as u8 || self.inopblog >= u64::BITS as u8 {
                return Err(invalid_fs_data(format!(
                    "invalid XFS inode geometry agblklog={} inopblog={}",
                    self.agblklog, self.inopblog
                )));
            }
            let ag_num = ino >> shift;
            let low_bits = ino & ((1u64 << shift) - 1);
            let blk_num = low_bits >> self.inopblog;
            let ino_in_blk = low_bits & ((1u64 << self.inopblog) - 1);
            if ag_num >= u64::from(self._ag_count) || blk_num >= self._ag_blocks {
                return Err(invalid_fs_data(format!(
                    "inode {ino} outside XFS AG geometry (agno={ag_num} agbno={blk_num} agcount={} agblocks={})",
                    self._ag_count, self._ag_blocks
                )));
            }
            let fs_blockno = ag_num
                .checked_mul(self._ag_blocks)
                .and_then(|base| base.checked_add(blk_num))
                .ok_or_else(|| invalid_fs_data(format!("inode {ino} offset overflows")))?;
            let block_offset = self.block_to_offset(fs_blockno)?;
            let inode_delta = ino_in_blk
                .checked_mul(u64::from(self.inode_size))
                .ok_or_else(|| invalid_fs_data(format!("inode {ino} offset overflows")))?;
            return block_offset
                .checked_add(inode_delta)
                .ok_or_else(|| invalid_fs_data(format!("inode {ino} offset overflows")));
        }

        let inode_delta = (ino - 1)
            .checked_mul(u64::from(self.inode_size))
            .ok_or_else(|| invalid_fs_data(format!("inode {ino} offset overflows")))?;
        self.block_to_offset(self.inode_base_block)?
            .checked_add(inode_delta)
            .ok_or_else(|| invalid_fs_data(format!("inode {ino} offset overflows")))
    }

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
                    "inode {ino} outside synthetic fixture range"
                )));
            }
        }
        self.read_inode_at_offset(self.inode_offset(ino)?)
    }

    pub(crate) fn inode_core_size(inode: &[u8]) -> usize {
        if inode.len() > di_off::VERSION && inode[di_off::VERSION] == 3 {
            INODE_CORE_SIZE_V3
        } else {
            INODE_CORE_SIZE
        }
    }

    pub(crate) fn data_fork(inode: &[u8]) -> io::Result<&[u8]> {
        let core_size = Self::inode_core_size(inode);
        let format = *inode
            .get(di_off::FORMAT)
            .ok_or_else(|| invalid_fs_data("inode truncated before di_format"))?;
        let literal = inode.get(core_size..).ok_or_else(|| {
            invalid_fs_data(format!(
                "inode buffer of {} bytes is shorter than the v3 core ({core_size} bytes)",
                inode.len()
            ))
        })?;
        if format == FORMAT_LOCAL {
            return Ok(literal);
        }
        let forkoff_units = usize::from(
            *inode
                .get(di_off::FORKOFF)
                .ok_or_else(|| invalid_fs_data("inode truncated before di_forkoff"))?,
        );
        if forkoff_units == 0 {
            return Ok(literal);
        }
        let forkoff = forkoff_units.checked_mul(8).ok_or_else(|| {
            invalid_fs_data(format!("di_forkoff {forkoff_units} overflows bytes"))
        })?;
        literal.get(..forkoff).ok_or_else(|| {
            invalid_fs_data(format!(
                "di_forkoff {forkoff_units} ({forkoff} bytes) exceeds literal area {}",
                literal.len()
            ))
        })
    }

    pub(crate) fn max_inline_extents(inode: &[u8]) -> usize {
        Self::data_fork(inode)
            .map(|data_fork| data_fork.len() / BMBT_REC_SIZE)
            .unwrap_or(0)
    }

    pub(crate) fn nextents(inode: &[u8]) -> u32 {
        be_u32(inode, di_off::NEXTENTS)
    }

    pub(crate) fn decode_extent(rec: &[u8]) -> XfsExtent {
        let l0 = be_u64(rec, 0);
        let l1 = be_u64(rec, 8);
        XfsExtent {
            logical: (l0 >> 9) & ((1u64 << 54) - 1),
            start_block: ((l0 & 0x1FF) << 43) | (l1 >> 21),
            block_count: l1 & 0x1F_FFFF,
            unwritten: (l0 >> 63) != 0,
        }
    }

    /// Inode buffers shorter than the v1/v2 core cannot even address
    /// `di_format`/`di_size`; reject them before any field read.
    pub(crate) fn validate_inode_core_length(inode: &[u8]) -> io::Result<()> {
        if inode.len() < INODE_CORE_SIZE {
            return Err(invalid_fs_data(format!(
                "inode buffer of {} bytes is shorter than the inode core ({INODE_CORE_SIZE} bytes)",
                inode.len()
            )));
        }
        Ok(())
    }

    pub(crate) fn inode_format(inode: &[u8]) -> io::Result<u8> {
        inode
            .get(di_off::FORMAT)
            .copied()
            .ok_or_else(|| invalid_fs_data("inode truncated before di_format"))
    }

    /// Reject a declared `di_size` that cannot exist on this filesystem or
    /// that would buffer more than the parser maximum, so raw on-disk sizes
    /// never drive unbounded allocation or zero-fill.
    pub(crate) fn validate_declared_file_size(&self, size: u64) -> io::Result<()> {
        let capacity = self
            .dblocks
            .checked_mul(self.block_size)
            .ok_or_else(|| invalid_fs_data("XFS filesystem capacity overflows"))?;
        if size > capacity {
            return Err(invalid_fs_data(format!(
                "declared file size {size} exceeds XFS filesystem capacity {capacity}"
            )));
        }
        if size > MAX_BUFFERED_FILE_BYTES {
            return Err(invalid_fs_data(format!(
                "declared file size {size} exceeds the {MAX_BUFFERED_FILE_BYTES} byte buffered-read limit"
            )));
        }
        Ok(())
    }

    pub(crate) fn validate_inode_magic(inode: &[u8]) -> io::Result<()> {
        if inode.len() < 2 {
            return Err(invalid_fs_data("inode buffer too short"));
        }
        let magic = be_u16(inode, di_off::MAGIC);
        if magic != XFS_INODE_MAGIC {
            return Err(invalid_fs_data(format!(
                "invalid inode magic 0x{magic:04X}, expected 0x{XFS_INODE_MAGIC:04X}"
            )));
        }
        Ok(())
    }

    pub(crate) fn inode_is_dir(inode: &[u8]) -> bool {
        // Compare the complete POSIX file-type field; sockets and symlinks
        // also carry the directory bit when tested with a bare bitwise AND.
        (be_u16(inode, di_off::MODE) & S_IFMT) == S_IFDIR
    }
}

fn aligned_inode_cache_page_size(block_size: u64, preferred: usize) -> u64 {
    let Ok(preferred) = u64::try_from(preferred) else {
        return block_size;
    };
    if preferred >= block_size && preferred % block_size == 0 {
        preferred.min(1024 * 1024)
    } else {
        block_size
    }
}
