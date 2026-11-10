//! Recording that a tree block is allocated.
//!
//! [`crate::block_group`] finds an address that is free.  Nothing yet
//! makes it stop being free, so asking twice returns the same answer
//! twice and the second block lands on the first.  This writes the item
//! that closes that gap.
//!
//! # The shape, measured
//!
//! On a filesystem with `SKINNY_METADATA` -- the default for years -- an
//! allocated tree block is one 33-byte item, and every one of the 2,603
//! on the deep fixture has the same layout:
//!
//! ```text
//!   key   objectid = the block's address
//!         type     = METADATA_ITEM (169)
//!         offset   = the block's LEVEL, not its length
//!
//!   body   0  refs        u64  = 1 for a block one tree points at
//!          8  generation  u64  the transaction that allocated it
//!         16  flags       u64  = 2, TREE_BLOCK
//!         24  ref type    u8   = 0xb0, TREE_BLOCK_REF
//!         25  ref offset  u64  the objectid of the tree that owns it
//! ```
//!
//! The last nine bytes are an inline back reference.  Btrfs can store a
//! reference either inside the extent item or as a separate keyed item,
//! and for a tree block with a single owner it is always inline -- which
//! is why 33 bytes is not a coincidence but the only size observed.
//!
//! # Without the feature
//!
//! Without `SKINNY_METADATA` the same fact is recorded as an
//! `EXTENT_ITEM` whose offset really is a length, carrying an extra
//! `btrfs_tree_block_info` between the flags and the reference.  That
//! shape is refused rather than approximated: a 33-byte item written
//! where a 51-byte one belongs is not a smaller version of the right
//! answer.

use crate::chunk::{key_type, DiskKey};
use crate::error::{Error, Result};
use crate::superblock::Superblock;

/// `BTRFS_FEATURE_INCOMPAT_SKINNY_METADATA`, re-exported under the name
/// this module uses. The one copy is `superblock::incompat`.
pub use crate::superblock::incompat::SKINNY_METADATA as INCOMPAT_SKINNY_METADATA;

/// `BTRFS_EXTENT_FLAG_DATA` — the extent holds file data.
pub const EXTENT_FLAG_DATA: u64 = 1 << 0;
/// `BTRFS_EXTENT_FLAG_TREE_BLOCK` — the extent holds a tree block.
pub const EXTENT_FLAG_TREE_BLOCK: u64 = 1 << 1;

/// `BTRFS_TREE_BLOCK_REF_KEY`, as an inline reference type.
pub const TREE_BLOCK_REF: u8 = 176;

/// The size of the item this module writes.
pub const SKINNY_METADATA_ITEM_SIZE: usize = 33;

/// Byte offsets within the item.
pub mod offsets {
    /// How many references point at the block.
    pub const REFS: usize = 0;
    /// The transaction that allocated it.
    pub const GENERATION: usize = 8;
    /// What kind of extent it is.
    pub const FLAGS: usize = 16;
    /// The inline reference's type byte.
    pub const REF_TYPE: usize = 24;
    /// The inline reference's payload — for a tree block reference,
    /// the objectid of the owning tree.
    pub const REF_OFFSET: usize = 25;
}

/// One newly allocated tree block, as the extent tree must record it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TreeBlockAllocation {
    /// Where the block is.
    pub bytenr: u64,
    /// Its height above the leaves. Filed in the KEY, not the body.
    pub level: u8,
    /// The transaction allocating it.
    pub generation: u64,
    /// The objectid of the tree the block belongs to.
    pub owner: u64,
}

impl TreeBlockAllocation {
    /// The key this allocation is filed under.
    ///
    /// The offset is the level. It is not a length, and a reader that
    /// treats it as one frees every tree block on the filesystem — see
    /// [`crate::block_group`].
    pub fn key(&self) -> DiskKey {
        DiskKey {
            objectid: self.bytenr,
            key_type: key_type::METADATA_ITEM,
            offset: self.level as u64,
        }
    }

    /// The item body: an extent item with one inline reference.
    ///
    /// `refs` is 1. A block that more than one tree points at is the
    /// result of a snapshot sharing it, which is not something
    /// allocating a new block can produce.
    pub fn body(&self) -> [u8; SKINNY_METADATA_ITEM_SIZE] {
        let mut out = [0u8; SKINNY_METADATA_ITEM_SIZE];
        out[offsets::REFS..offsets::REFS + 8].copy_from_slice(&1u64.to_le_bytes());
        out[offsets::GENERATION..offsets::GENERATION + 8]
            .copy_from_slice(&self.generation.to_le_bytes());
        out[offsets::FLAGS..offsets::FLAGS + 8]
            .copy_from_slice(&EXTENT_FLAG_TREE_BLOCK.to_le_bytes());
        out[offsets::REF_TYPE] = TREE_BLOCK_REF;
        out[offsets::REF_OFFSET..offsets::REF_OFFSET + 8]
            .copy_from_slice(&self.owner.to_le_bytes());
        out
    }
}

/// The key and body recording `alloc`, checked against the filesystem's
/// features first.
///
/// # Errors
///
/// [`Error::UnsupportedFeature`] when the filesystem does not have
/// `SKINNY_METADATA`, because the item it wants is a different size and
/// carries a field this does not write.
pub fn record_tree_block(
    sb: &Superblock,
    alloc: TreeBlockAllocation,
) -> Result<(DiskKey, [u8; SKINNY_METADATA_ITEM_SIZE])> {
    if sb.incompat_flags & INCOMPAT_SKINNY_METADATA == 0 {
        return Err(Error::UnsupportedFeature(
            "this filesystem records tree blocks as EXTENT_ITEMs with a \
             btrfs_tree_block_info, which is a longer item than this writes; recording an \
             allocation without SKINNY_METADATA is not implemented"
                .to_string(),
        ));
    }
    Ok((alloc.key(), alloc.body()))
}

/// A block group's `used` after taking `bytes` out of it.
///
/// Trivial arithmetic with one thing worth refusing: taking more than is
/// free. That is the arithmetic an allocator does after handing out an
/// address it should not have, and letting it wrap produces a `used`
/// near `u64::MAX` — which reads as a full group and quietly stops the
/// filesystem allocating anything there again.
///
/// # Errors
///
/// [`Error::UnsupportedFeature`] when the group does not have that much
/// free.
pub fn used_after_allocating(group: &crate::block_group::BlockGroup, bytes: u64) -> Result<u64> {
    if bytes > group.free() {
        return Err(Error::UnsupportedFeature(format!(
            "allocating {bytes} bytes in the block group at {} would take it past its own \
             length: {} of {} bytes are already used",
            group.start, group.used, group.length
        )));
    }
    Ok(group.used + bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block_group::BlockGroup;
    use crate::chunk::block_group as bg_flags;

    /// The level goes in the key and nowhere else.
    #[test]
    fn the_level_is_filed_in_the_key_not_the_body() {
        let alloc = TreeBlockAllocation {
            bytenr: 30_425_088,
            level: 2,
            generation: 7,
            owner: 5,
        };
        let key = alloc.key();
        assert_eq!(key.objectid, 30_425_088);
        assert_eq!(key.key_type, key_type::METADATA_ITEM);
        assert_eq!(key.offset, 2, "the offset is the level");

        // Nothing in the body encodes the level, so a body built at one
        // level equals a body built at another.
        let other = TreeBlockAllocation { level: 0, ..alloc };
        assert_eq!(alloc.body(), other.body());
    }

    /// The 33 bytes, field by field, against the layout measured on a
    /// real filesystem.
    #[test]
    fn the_body_is_the_thirty_three_bytes_the_kernel_writes() {
        let body = TreeBlockAllocation {
            bytenr: 22_024_192,
            level: 0,
            generation: 6,
            owner: 3,
        }
        .body();

        assert_eq!(body.len(), 33);
        assert_eq!(&body[0..8], &1u64.to_le_bytes(), "refs");
        assert_eq!(&body[8..16], &6u64.to_le_bytes(), "generation");
        assert_eq!(&body[16..24], &2u64.to_le_bytes(), "flags = TREE_BLOCK");
        assert_eq!(body[24], 0xb0, "an inline TREE_BLOCK_REF");
        assert_eq!(&body[25..33], &3u64.to_le_bytes(), "the owning tree");
    }

    /// A tree block is not data, and the two flags are not
    /// interchangeable.
    #[test]
    fn a_tree_block_is_not_flagged_as_data() {
        let body = TreeBlockAllocation {
            bytenr: 4096,
            level: 1,
            generation: 1,
            owner: 5,
        }
        .body();
        let flags = u64::from_le_bytes(body[16..24].try_into().unwrap());
        assert_eq!(flags & EXTENT_FLAG_TREE_BLOCK, EXTENT_FLAG_TREE_BLOCK);
        assert_eq!(flags & EXTENT_FLAG_DATA, 0);
    }

    fn group(used: u64, length: u64) -> BlockGroup {
        BlockGroup {
            start: 22_020_096,
            length,
            used,
            flags: bg_flags::METADATA,
        }
    }

    /// Usage goes up by what was taken.
    #[test]
    fn allocating_moves_the_groups_used_count_up() {
        let g = group(16384, 1 << 20);
        assert_eq!(used_after_allocating(&g, 4096).unwrap(), 20480);
        // Exactly filling the group is legal.
        let g = group((1 << 20) - 4096, 1 << 20);
        assert_eq!(used_after_allocating(&g, 4096).unwrap(), 1 << 20);
    }

    /// Taking more than is free is refused rather than wrapped.
    #[test]
    fn overfilling_a_group_is_refused() {
        let g = group(1 << 20, 1 << 20);
        assert!(used_after_allocating(&g, 4096).is_err(), "a full group");

        let g = group((1 << 20) - 1, 1 << 20);
        assert!(
            used_after_allocating(&g, 4096).is_err(),
            "one free byte is not four thousand"
        );
    }
}
