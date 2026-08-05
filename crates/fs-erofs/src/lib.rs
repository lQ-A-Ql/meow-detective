//! Read-only EROFS access for Android evidence sources.

mod checksum;
mod chunk;
mod directory;
mod error;
mod file;
mod filesystem;
mod inode;
mod io;
mod reader;
mod superblock;

pub use error::{ErofsError, Result};
pub use reader::ErofsReader;
pub use superblock::ErofsSuperblock;

pub const EROFS_MAGIC: u32 = 0xe0f5_e1e2;
pub const EROFS_BLOCK_SIZE: usize = 4096;
