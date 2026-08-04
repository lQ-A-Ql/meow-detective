use crate::io::{read_u16, read_u32, read_u64};
use crate::{F2fsError, Result, F2FS_BLOCK_SIZE};

pub(crate) const MODE_TYPE_MASK: u16 = 0xf000;
pub(crate) const MODE_DIRECTORY: u16 = 0x4000;
pub(crate) const MODE_REGULAR: u16 = 0x8000;
pub(crate) const MODE_SYMLINK: u16 = 0xa000;

const INLINE_XATTR: u8 = 0x01;
const INLINE_DATA: u8 = 0x02;
const INLINE_DENTRY: u8 = 0x04;
const EXTRA_ATTR: u8 = 0x20;
const ENCRYPTED_INODE_FLAG: u32 = 0x0000_0800;
const INODE_ADDRESS_OFFSET: usize = 360;
const INODE_ADDRESS_WORDS: usize = 923;
const INODE_NID_OFFSET: usize = 4052;
const NODE_FOOTER_OFFSET: usize = 4072;

#[derive(Debug, Clone)]
pub(crate) struct F2fsInode {
    pub(crate) nid: u32,
    pub(crate) mode: u16,
    pub(crate) size: u64,
    pub(crate) inline_flags: u8,
    pub(crate) flags: u32,
    pub(crate) data_blocks: Vec<u32>,
    inline_data: Option<Vec<u8>>,
}

impl F2fsInode {
    pub(crate) fn parse(bytes: &[u8], expected_nid: u32, expected_inode: u32) -> Result<Self> {
        if bytes.len() != F2FS_BLOCK_SIZE {
            return Err(F2fsError::Invalid(format!(
                "inode node {expected_nid} is not one F2FS block"
            )));
        }
        let footer_nid = read_u32(bytes, NODE_FOOTER_OFFSET, "node footer nid")?;
        let footer_inode = read_u32(bytes, NODE_FOOTER_OFFSET + 4, "node footer inode")?;
        if footer_nid != expected_nid || footer_inode != expected_inode {
            return Err(F2fsError::Invalid(format!(
                "node footer mismatch for nid {expected_nid}: nid={footer_nid}, ino={footer_inode}"
            )));
        }
        let inline_flags = *bytes
            .get(3)
            .ok_or_else(|| F2fsError::Invalid("truncated inode inline flags".to_string()))?;
        let extra_size = if inline_flags & EXTRA_ATTR != 0 {
            read_u16(bytes, INODE_ADDRESS_OFFSET, "inode extra attribute size")? as usize
        } else {
            0
        };
        if extra_size % 4 != 0 || extra_size > INODE_NID_OFFSET - INODE_ADDRESS_OFFSET {
            return Err(F2fsError::Invalid(format!(
                "inode {expected_nid} has invalid extra attribute size {extra_size}"
            )));
        }
        let inline_xattr_words = if inline_flags & INLINE_XATTR == 0 {
            0
        } else if inline_flags & EXTRA_ATTR != 0 {
            read_u16(bytes, INODE_ADDRESS_OFFSET + 2, "inline xattr words")? as usize
        } else {
            50
        };
        let address_words = INODE_ADDRESS_WORDS
            .checked_sub(extra_size / 4)
            .and_then(|count| count.checked_sub(inline_xattr_words))
            .ok_or_else(|| F2fsError::Invalid("inode address capacity underflows".to_string()))?;
        let address_start = INODE_ADDRESS_OFFSET + extra_size;
        let inline_length = address_words
            .checked_sub(1)
            .and_then(|words| words.checked_mul(4))
            .ok_or_else(|| F2fsError::Invalid("inline data capacity underflows".to_string()))?;
        let inline_start = address_start
            .checked_add(4)
            .ok_or_else(|| F2fsError::Invalid("inline data offset overflows".to_string()))?;
        let inline_end = inline_start
            .checked_add(inline_length)
            .ok_or_else(|| F2fsError::Invalid("inline data range overflows".to_string()))?;
        let inline_data = if inline_flags & (INLINE_DATA | INLINE_DENTRY) != 0 {
            Some(
                bytes
                    .get(inline_start..inline_end)
                    .ok_or_else(|| {
                        F2fsError::Invalid("inline inode data exceeds its node".to_string())
                    })?
                    .to_vec(),
            )
        } else {
            None
        };
        let mut data_blocks = Vec::with_capacity(address_words);
        for index in 0..address_words {
            data_blocks.push(read_u32(
                bytes,
                address_start + index * 4,
                "inode data block address",
            )?);
        }
        Ok(Self {
            nid: expected_nid,
            mode: read_u16(bytes, 0, "inode mode")?,
            size: read_u64(bytes, 16, "inode size")?,
            inline_flags,
            flags: read_u32(bytes, 80, "inode flags")?,
            data_blocks,
            inline_data,
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

    pub(crate) fn is_encrypted(&self) -> bool {
        self.flags & ENCRYPTED_INODE_FLAG != 0
    }

    pub(crate) fn require_external_data(&self, operation: &str) -> Result<()> {
        self.require_unencrypted(operation)?;
        let disallowed = if self.is_directory() {
            INLINE_DENTRY
        } else {
            INLINE_DATA
        };
        if self.inline_flags & disallowed != 0 {
            return Err(F2fsError::Unsupported(format!(
                "inline {operation} for inode {}",
                self.nid
            )));
        }
        Ok(())
    }

    pub(crate) fn require_unencrypted(&self, operation: &str) -> Result<()> {
        if self.is_encrypted() {
            return Err(F2fsError::Unsupported(format!(
                "encrypted {operation} for inode {}",
                self.nid
            )));
        }
        Ok(())
    }

    pub(crate) fn inline_file_data(&self) -> Option<&[u8]> {
        (self.inline_flags & INLINE_DATA != 0)
            .then_some(self.inline_data.as_deref())
            .flatten()
    }

    pub(crate) fn inline_directory_data(&self) -> Option<&[u8]> {
        (self.inline_flags & INLINE_DENTRY != 0)
            .then_some(self.inline_data.as_deref())
            .flatten()
    }

    pub(crate) fn required_blocks(&self) -> Result<usize> {
        let blocks = self.size.div_ceil(F2FS_BLOCK_SIZE as u64);
        let blocks = usize::try_from(blocks)
            .map_err(|_| F2fsError::Unsupported("file block count exceeds usize".to_string()))?;
        if blocks > self.data_blocks.len() {
            return Err(F2fsError::Unsupported(format!(
                "indirect node traversal for inode {}",
                self.nid
            )));
        }
        Ok(blocks)
    }
}
