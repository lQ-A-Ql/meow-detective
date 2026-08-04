//! Read-only F2FS filesystem access for Android evidence sources.

mod checkpoint;
mod checksum;
mod directory;
mod error;
mod file;
mod filesystem;
mod inode;
mod io;
mod nat;
mod node;
mod reader;
mod superblock;

pub use error::{F2fsError, Result};
pub use reader::F2fsReader;
pub use superblock::{F2fsSuperblock, SuperblockCopy};

pub const F2FS_MAGIC: u32 = 0xf2f5_2010;
pub const F2FS_BLOCK_SIZE: usize = 4096;
