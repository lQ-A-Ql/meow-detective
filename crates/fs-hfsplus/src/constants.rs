//! HFS+ on-disk constants.
//!
//! Many offsets and record-type constants are declared for completeness even
//! when not yet exercised by the current reader code path.  Unused constants
//! that document the on-disk format are marked with `#[allow(dead_code)]`.

/// Volume header magic: `H+` as big-endian u16.
pub(crate) const HFSPLUS_SIGNATURE: u16 = 0x482B;
/// HFSX (case-sensitive) volume header magic: `HX` as big-endian u16.
pub(crate) const HFSX_SIGNATURE: u16 = 0x4858;
/// Volume header offset from start of the partition / reader.
pub(crate) const VOLUME_HEADER_OFFSET: u64 = 1024;
/// Volume header size in bytes.
pub(crate) const VOLUME_HEADER_SIZE: usize = 512;

// Volume header field offsets (from start of volume header).
pub(crate) const VH_SIGNATURE: usize = 0x00;
#[allow(dead_code)]
pub(crate) const VH_VERSION: usize = 0x02;
#[allow(dead_code)]
pub(crate) const VH_CREATE_DATE: usize = 0x18;
#[allow(dead_code)]
pub(crate) const VH_MODIFY_DATE: usize = 0x1C;
#[allow(dead_code)]
pub(crate) const VH_FILE_COUNT: usize = 0x22;
#[allow(dead_code)]
pub(crate) const VH_FOLDER_COUNT: usize = 0x26;
pub(crate) const VH_BLOCK_SIZE: usize = 0x28;
pub(crate) const VH_TOTAL_BLOCKS: usize = 0x2C;
pub(crate) const VH_FREE_BLOCKS: usize = 0x30;
#[allow(dead_code)]
pub(crate) const VH_NEXT_ALLOCATION: usize = 0x34;
#[allow(dead_code)]
pub(crate) const VH_NEXT_CATALOG_ID: usize = 0x48;
pub(crate) const VH_CATALOG_FILE: usize = 0xE0; // HFSPlusForkData for the catalog B-tree

// HFSPlusForkData offsets.
pub(crate) const FORK_LOGICAL_SIZE: usize = 0x00;
pub(crate) const FORK_TOTAL_BLOCKS: usize = 0x0C;
pub(crate) const FORK_EXTENTS: usize = 0x10;

// HFSPlusExtentDescriptor size.
pub(crate) const EXTENT_DESC_SIZE: usize = 8; // startBlock(u32) + blockCount(u32)

// B-tree node descriptor offsets.
pub(crate) const BT_F_LINK: usize = 0x00;
pub(crate) const BT_B_LINK: usize = 0x04;
pub(crate) const BT_KIND: usize = 0x08;
pub(crate) const BT_HEIGHT: usize = 0x09;
pub(crate) const BT_NUM_RECORDS: usize = 0x0A;
#[allow(dead_code)]
pub(crate) const BT_RESERVED: usize = 0x0C;
pub(crate) const BT_NODE_DESC_SIZE: usize = 0x0E;

// B-tree node kinds.
pub(crate) const BT_LEAF_NODE: u8 = 0x00;
pub(crate) const BT_INDEX_NODE: u8 = 0x01;
pub(crate) const BT_HEADER_NODE: u8 = 0x02;

// B-tree header record (first record in the header node) offsets.
// Record data starts after keyLength (u16) + parentCNID(u32) + nameLen(u16=0) = 8 bytes.
// But the header node's keys are special — the header record has a fixed format.
#[allow(dead_code)]
pub(crate) const BT_HEADER_TREE_DEPTH: usize = 0x00; // u16
pub(crate) const BT_HEADER_ROOT_NODE: usize = 0x02; // u32
#[allow(dead_code)]
pub(crate) const BT_HEADER_LEAF_RECORDS: usize = 0x06; // u32
#[allow(dead_code)]
pub(crate) const BT_HEADER_FIRST_LEAF: usize = 0x0A; // u32
#[allow(dead_code)]
pub(crate) const BT_HEADER_LAST_LEAF: usize = 0x0E; // u32
pub(crate) const BT_HEADER_NODE_SIZE: usize = 0x12; // u16
#[allow(dead_code)]
pub(crate) const BT_HEADER_MAX_KEY_LEN: usize = 0x14; // u16
pub(crate) const BT_HEADER_TOTAL_NODES: usize = 0x16; // u32
#[allow(dead_code)]
pub(crate) const BT_HEADER_FREE_LIST: usize = 0x1A; // u32
                                                    // Catalog record types.
pub(crate) const RECORD_TYPE_FOLDER: i16 = 0x0001;
pub(crate) const RECORD_TYPE_FILE: i16 = 0x0002;
#[allow(dead_code)]
pub(crate) const RECORD_TYPE_FOLDER_THREAD: i16 = 0x0003;
#[allow(dead_code)]
pub(crate) const RECORD_TYPE_FILE_THREAD: i16 = 0x0004;

// HFSPlusCatalogFolder field offsets (from start of record data).
#[allow(dead_code)]
pub(crate) const FOLDER_RECORD_TYPE: usize = 0x00;
#[allow(dead_code)]
pub(crate) const FOLDER_FLAGS: usize = 0x02;
#[allow(dead_code)]
pub(crate) const FOLDER_VALENCE: usize = 0x04;
pub(crate) const FOLDER_ID: usize = 0x08;
pub(crate) const FOLDER_CREATE_DATE: usize = 0x0C;
pub(crate) const FOLDER_CONTENT_MOD_DATE: usize = 0x10;
#[allow(dead_code)]
pub(crate) const FOLDER_ATTR_MOD_DATE: usize = 0x14;
pub(crate) const FOLDER_ACCESS_DATE: usize = 0x18;
#[allow(dead_code)]
pub(crate) const FOLDER_BACKUP_DATE: usize = 0x1C;
pub(crate) const FOLDER_PERMISSIONS: usize = 0x20;
#[allow(dead_code)]
pub(crate) const FOLDER_USER_INFO: usize = 0x30;
#[allow(dead_code)]
pub(crate) const FOLDER_FINDER_INFO: usize = 0x40;
#[allow(dead_code)]
pub(crate) const FOLDER_TEXT_ENCODING: usize = 0x50;
pub(crate) const FOLDER_RECORD_SIZE: usize = 0x58;

// HFSPlusBSDInfo offsets.
#[allow(dead_code)]
pub(crate) const BSDINFO_OWNER_ID: usize = 0x00;
#[allow(dead_code)]
pub(crate) const BSDINFO_GROUP_ID: usize = 0x04;
pub(crate) const BSDINFO_FILE_MODE: usize = 0x0A;
pub(crate) const BSDINFO_SPECIAL: usize = 0x0C;

// HFSPlusCatalogFile field offsets (from start of record data).
#[allow(dead_code)]
pub(crate) const FILE_RECORD_TYPE: usize = 0x00;
#[allow(dead_code)]
pub(crate) const FILE_FLAGS: usize = 0x02;
#[allow(dead_code)]
pub(crate) const FILE_RESERVED1: usize = 0x04;
pub(crate) const FILE_ID: usize = 0x08;
pub(crate) const FILE_CREATE_DATE: usize = 0x0C;
pub(crate) const FILE_CONTENT_MOD_DATE: usize = 0x10;
#[allow(dead_code)]
pub(crate) const FILE_ATTR_MOD_DATE: usize = 0x14;
pub(crate) const FILE_ACCESS_DATE: usize = 0x18;
#[allow(dead_code)]
pub(crate) const FILE_BACKUP_DATE: usize = 0x1C;
pub(crate) const FILE_PERMISSIONS: usize = 0x20;
#[allow(dead_code)]
pub(crate) const FILE_USER_INFO: usize = 0x30;
#[allow(dead_code)]
pub(crate) const FILE_FINDER_INFO: usize = 0x40;
#[allow(dead_code)]
pub(crate) const FILE_TEXT_ENCODING: usize = 0x50;
#[allow(dead_code)]
pub(crate) const FILE_RESERVED2: usize = 0x54;
pub(crate) const FILE_DATA_FORK: usize = 0x58;

// BSD file-mode bits.
#[allow(dead_code)]
pub(crate) const S_IFMT: u16 = 0xF000;
#[allow(dead_code)]
pub(crate) const S_IFLNK: u16 = 0xA000;

// Finder info type-code offset for symlink detection.
#[allow(dead_code)]
pub(crate) const FINDER_TYPE_OFFSET: usize = 0x00; // first 4 bytes: file type code

/// The Mac epoch is 1904-01-01 UTC.  Unix epoch is 1970-01-01 UTC.
/// Offset in seconds: 2082844800.
pub(crate) const MAC_TO_UNIX_EPOCH_OFFSET: i64 = 2082844800;
