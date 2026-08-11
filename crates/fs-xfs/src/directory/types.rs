use std::io;

use crate::inode::XfsInodeMetadata;

pub(crate) const DIR2_SF_HDR_8: usize = 10;
pub(super) const DIR2_SF_HDR_4: usize = 6;
pub(crate) const XFS_DIR3_BLOCK_MAGIC: u32 = 0x5844_4233;
pub(crate) const XFS_DIR2_BLOCK_MAGIC: u32 = 0x5844_3242;
pub(super) const XFS_DIR2_BLOCK_MAGIC_LEGACY: u32 = 0x5844_4232;
pub(crate) const XFS_DIR3_DATA_MAGIC: u32 = 0x5844_4433;
pub(crate) const XFS_DIR2_DATA_MAGIC: u32 = 0x5844_3244;
pub(super) const XFS_DIR2_DATA_MAGIC_LEGACY: u32 = 0x5844_4432;
pub(crate) const XFS_DIR3_DATA_HDR_SIZE: usize = 64;
pub(crate) const XFS_DIR2_DATA_HDR_SIZE: usize = 16;
pub(crate) const XFS_DIR2_FREE_TAG: u16 = 0xFFFF;
pub(super) const XFS_DIR2_SPACE_SIZE: u64 = 1u64 << 35;
pub(super) const XFS_DIR2_DATA_SPACE: u64 = 0;
pub(super) const XFS_DIR2_LEAF_SPACE: u64 = 1;
pub(super) const XFS_DIR3_FT_UNKNOWN: u8 = 0;
pub(crate) const XFS_DIR3_FT_DIR: u8 = 2;
pub(super) const XFS_DIR3_FT_MAX: u8 = 9;
pub(crate) const XFS_DIR2_DATA_ALIGN: usize = 8;
pub(super) const XFS_DIR2_DATA_ENTRY_FIXED_SIZE: usize = 9;
pub(super) const XFS_DIR2_DATA_ENTRY_TAG_SIZE: usize = 2;
pub(super) const XFS_DIR3_DATA_ENTRY_FTYPE_SIZE: usize = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct XfsDirectoryEntry {
    pub(crate) name: String,
    pub(crate) inode: u64,
    pub(crate) ftype: Option<u8>,
}

pub(crate) struct XfsResolvedDirectoryEntry {
    pub(crate) name: String,
    pub(crate) inode: u64,
    pub(crate) is_dir: bool,
    pub(crate) metadata: Option<XfsInodeMetadata>,
}

#[derive(Default)]
pub(super) struct DirectoryReadOutcome {
    pub(super) entries: Vec<XfsDirectoryEntry>,
    pub(super) first_error: Option<io::Error>,
    pub(super) saw_recoverable_block: bool,
    pub(super) scanned_bytes: u64,
}

impl DirectoryReadOutcome {
    pub(super) fn record_error(&mut self, error: io::Error) {
        if self.first_error.is_none() {
            self.first_error = Some(error);
        }
    }

    pub(super) fn should_try_residual_shortform(&self) -> bool {
        self.saw_recoverable_block
    }

    pub(super) fn into_result(self) -> io::Result<Vec<XfsDirectoryEntry>> {
        match (self.entries.is_empty(), self.first_error) {
            (false, _) => Ok(self.entries),
            (true, Some(error)) => Err(error),
            (true, None) => Ok(self.entries),
        }
    }
}
