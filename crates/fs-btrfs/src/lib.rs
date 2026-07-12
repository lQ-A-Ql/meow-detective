//! Btrfs filesystem reader.
//!
//! The crate exposes a read-only `FileSystemReader` implementation. Production
//! responsibilities are split by on-disk format, B-tree traversal, directory
//! resolution, extent reading, and filesystem facade behavior.

mod btree;
mod directory;
mod extents;
mod filesystem;
mod format;
mod reader;
pub mod snapshot;
mod types;
mod types_reader;

pub use types::BtrfsSubvol;
pub use types_reader::BtrfsReader;

pub(crate) use format::*;
pub(crate) use types::BtrfsHeader;

#[cfg(test)]
#[path = "../tests/unit/btrfs.rs"]
mod tests;
