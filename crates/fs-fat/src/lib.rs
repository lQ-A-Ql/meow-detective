//! Read-only FAT12, FAT16, and FAT32 filesystem support.

mod boot_sector;
mod cluster_chain;
mod directory;
mod reader;
mod types;

pub use types::FatReader;

#[cfg(test)]
#[path = "../tests/unit/lib.rs"]
mod tests;
