//! Read-only XFS filesystem reader.
//!
//! The crate preserves XFS allocation-group geometry, inode addressing,
//! directory decoding, extent/B+tree reads, and log recovery behind the
//! [`XfsReader`] facade.

mod directory;
mod extents;
mod filesystem;
mod geometry;
mod inode;
mod inode_cache;
mod locator;
mod reader;
mod repair;
mod rewrite;

pub mod log;
pub use repair::{XfsLogClearPlan, XfsRepairPatch};
pub use rewrite::{XfsFileRewritePatch, XfsFileRewritePlan};

pub use reader::XfsReader;

pub(crate) use reader::{
    be_u16, be_u32, be_u64, di_off, XfsExtent, BMA3_MAGIC, BMAP_MAGIC, BMBT_BLOCK_HDR_SIZE,
    BMBT_CRC_BLOCK_HDR_SIZE, BMBT_REC_SIZE, BMBT_SHORT_ROOT_HDR_SIZE, FORMAT_BTREE, FORMAT_EXTENTS,
    FORMAT_LOCAL, INODE_CORE_SIZE, INODE_CORE_SIZE_V3, MAX_BMBT_TREE_DEPTH,
    MAX_BUFFERED_FILE_BYTES, MAX_DIRECTORY_SCAN_BYTES,
};
