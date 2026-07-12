pub(crate) const BTRFS_SUPERBLOCK_OFFSET: u64 = 0x10000;
pub(crate) const BTRFS_MAGIC: &[u8; 8] = b"_BHRfS_M";
pub(crate) const BTRFS_HEADER_SIZE: usize = 101;
pub(crate) const LEAF_ITEM_SIZE: usize = 25;
pub(crate) const INTERNAL_ITEM_SIZE: usize = 33;
pub(crate) const KEY_SIZE: usize = 17;

pub(crate) const INODE_ITEM_KEY: u8 = 1;
pub(crate) const DIR_ITEM_KEY: u8 = 84;
pub(crate) const DIR_INDEX_KEY: u8 = 96;
pub(crate) const EXTENT_DATA_KEY: u8 = 108;
pub(crate) const ROOT_ITEM_KEY: u8 = 132;
pub(crate) const ROOT_BACKREF_KEY: u8 = 144;
pub(crate) const CHUNK_ITEM_KEY: u8 = 228;

pub(crate) const FS_TREE_OBJECTID: u64 = 5;
pub(crate) const FIRST_FREE_OBJECTID: u64 = 256;

pub(crate) const FT_DIR: u8 = 2;
pub(crate) const _FT_SYMLINK: u8 = 7;

pub(crate) const EXTENT_INLINE: u8 = 0;
