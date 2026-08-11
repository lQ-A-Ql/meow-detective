//! Read-only FAT12, FAT16, and FAT32 filesystem support.

mod boot_sector;
mod cluster_chain;
mod directory;
mod esp_fallback;
mod reader;
mod types;

pub use esp_fallback::{install_efi_fallback, EspFallbackInstall, FatBlockIo};
pub use types::FatReader;

#[cfg(test)]
#[path = "../tests/unit/lib.rs"]
mod tests;
