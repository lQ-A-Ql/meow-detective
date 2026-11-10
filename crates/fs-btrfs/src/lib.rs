//! Read-only Btrfs filesystem integration.
//!
//! On-disk decoding, checksum verification, subvolume crossing, and extent
//! decompression are provided by the MIT-licensed `rust-fs-btrfs` parser.
//! This crate adapts the application's evidence reader to its block-device
//! interface and projects its filesystem model onto `FileSystemReader`.

mod filesystem;
mod reader;
pub mod snapshot;
mod types_reader;

pub use types_reader::BtrfsReader;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BtrfsSubvol {
    pub id: u64,
    pub name: String,
    pub root_dirid: u64,
    pub tree_root_bytenr: u64,
}
