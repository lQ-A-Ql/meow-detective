use crate::io::{read_u16, read_u32, read_u64};
use crate::{ErofsError, Result};

pub(crate) const MODE_TYPE_MASK: u16 = 0xf000;
pub(crate) const MODE_DIRECTORY: u16 = 0x4000;
pub(crate) const MODE_REGULAR: u16 = 0x8000;
pub(crate) const MODE_SYMLINK: u16 = 0xa000;

const FORMAT_VERSION_MASK: u16 = 0x0001;
const FORMAT_LAYOUT_MASK: u16 = 0x000e;
const FORMAT_LAYOUT_SHIFT: u16 = 1;
const FORMAT_NLINK_ONE: u16 = 0x0010;
const LAYOUT_FLAT_PLAIN: u8 = 0;
const LAYOUT_COMPRESSED_FULL: u8 = 1;
const LAYOUT_FLAT_INLINE: u8 = 2;
const LAYOUT_COMPRESSED_COMPACT: u8 = 3;
const LAYOUT_CHUNK: u8 = 4;

#[derive(Debug, Clone)]
pub(crate) struct ErofsInode {
    pub(crate) nid: u64,
    pub(crate) mode: u16,
    pub(crate) size: u64,
    pub(crate) data_layout: u8,
    pub(crate) start_block: u64,
    pub(crate) source_offset: u64,
    pub(crate) inode_size: usize,
    pub(crate) xattr_size: usize,
}

impl ErofsInode {
    pub(crate) fn parse(bytes: &[u8], nid: u64, source_offset: u64) -> Result<Self> {
        let format = read_u16(bytes, 0, "inode format")?;
        if format & !0x001f != 0 {
            return Err(ErofsError::Unsupported(format!(
                "inode {nid} format flags {:#x}",
                format & !0x001f
            )));
        }
        let data_layout = ((format & FORMAT_LAYOUT_MASK) >> FORMAT_LAYOUT_SHIFT) as u8;
        if data_layout > LAYOUT_CHUNK {
            return Err(ErofsError::Unsupported(format!(
                "inode {nid} data layout {data_layout}"
            )));
        }
        let extended = format & FORMAT_VERSION_MASK != 0;
        let inode_size = if extended { 64 } else { 32 };
        if bytes.len() < inode_size {
            return Err(ErofsError::Invalid(format!("inode {nid} is truncated")));
        }
        let mode = read_u16(bytes, 4, "inode mode")?;
        let size = if extended {
            read_u64(bytes, 8, "extended inode size")?
        } else {
            u64::from(read_u32(bytes, 8, "compact inode size")?)
        };
        let start_low = u64::from(read_u32(bytes, 16, "inode start block")?);
        let start_high =
            if mode & MODE_TYPE_MASK != MODE_DIRECTORY && format & FORMAT_NLINK_ONE != 0 {
                u64::from(read_u16(bytes, 6, "inode start block high bits")?)
            } else {
                0
            };
        let xattr_count = usize::from(read_u16(bytes, 2, "inode xattr count")?);
        let xattr_size = if xattr_count == 0 {
            0
        } else {
            12usize
                .checked_add((xattr_count - 1).saturating_mul(4))
                .ok_or_else(|| ErofsError::Invalid("inode xattr size overflows".to_string()))?
        };
        Ok(Self {
            nid,
            mode,
            size,
            data_layout,
            start_block: start_low | start_high << 32,
            source_offset,
            inode_size,
            xattr_size,
        })
    }

    pub(crate) fn is_directory(&self) -> bool {
        self.mode & MODE_TYPE_MASK == MODE_DIRECTORY
    }

    pub(crate) fn is_regular(&self) -> bool {
        self.mode & MODE_TYPE_MASK == MODE_REGULAR
    }

    pub(crate) fn is_symlink(&self) -> bool {
        self.mode & MODE_TYPE_MASK == MODE_SYMLINK
    }

    pub(crate) fn require_uncompressed(&self, operation: &str) -> Result<()> {
        match self.data_layout {
            LAYOUT_FLAT_PLAIN | LAYOUT_FLAT_INLINE => Ok(()),
            LAYOUT_COMPRESSED_FULL | LAYOUT_COMPRESSED_COMPACT => Err(ErofsError::Unsupported(
                format!("compressed {operation} for inode {}", self.nid),
            )),
            LAYOUT_CHUNK => Err(ErofsError::Unsupported(format!(
                "chunk-based {operation} for inode {}",
                self.nid
            ))),
            _ => Err(ErofsError::Unsupported(format!(
                "unknown {operation} layout for inode {}",
                self.nid
            ))),
        }
    }

    pub(crate) fn inline_data_offset(&self) -> Result<Option<u64>> {
        if self.data_layout != LAYOUT_FLAT_INLINE {
            return Ok(None);
        }
        self.source_offset
            .checked_add(self.inode_size as u64)
            .and_then(|offset| offset.checked_add(self.xattr_size as u64))
            .map(Some)
            .ok_or_else(|| ErofsError::Invalid("inline data offset overflows".to_string()))
    }
}
