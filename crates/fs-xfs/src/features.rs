//! Superblock and inode feature-bit definitions plus volume capability queries.
//!
//! This module groups the on-disk feature masks this parser understands and
//! the read-only capability accessors reported on an opened
//! [`crate::reader::XfsReader`], keeping the reader's open/inode machinery
//! free of bit-flag tables.

use crate::reader::XfsReader;

/// sb_features2 bit for directory entry file-type (v2 format).
pub(crate) const XFS_SB_VERSION2_FTYPE: u32 = 0x0000_0200;

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

pub(crate) const XFS_SB_VERSION_NUMBITS: u16 = 0x000F;
pub(crate) const XFS_SB_VERSION_5: u16 = 5;

/// `di_flags2` bit 1: the inode's extents are shared with another file
/// (reflink/CoW). Rewriting shared extents in place would corrupt the
/// sibling file, so such inodes are rejected per-inode even on a volume that
/// advertises reflink support.
pub const XFS_DIFLAG2_REFLINK: u64 = 1 << 1;

impl XfsReader {
    /// Whether the superblock advertises reflink (CoW) support. Volume-level
    /// capability alone no longer blocks a rewrite: an individual inode must
    /// still pass the shared-extent check.
    pub fn volume_reflink_enabled(&self) -> bool {
        self.has_reflink
    }
}
