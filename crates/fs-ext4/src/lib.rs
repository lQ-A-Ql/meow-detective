//! ext4 filesystem reader.
//!
//! The crate exposes a read-only `FileSystemReader` implementation. Production
//! responsibilities are split across reader geometry, inode access, extent
//! traversal, directory resolution, and filesystem facade behavior.

mod block_cache;
mod directory;
mod extents;
mod filesystem;
mod format;
pub mod journal;
mod reader;
mod superblock;
mod write_map;

pub use reader::Ext4Reader;
pub use write_map::Ext4FileExtent;

#[cfg(test)]
#[path = "../tests/unit/ext4.rs"]
mod tests;
