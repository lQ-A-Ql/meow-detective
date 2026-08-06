//! NTFS filesystem reader.
//! Parses boot sector to locate $MFT, reads FILE records, enumerates file names.
//! Supports resident and non-resident attributes via data run parsing.

pub mod ads;
pub mod logfile;
pub mod mft_scanner;

mod attribute;
mod compression;
mod data_runs;
mod directory;
mod file_name;
mod file_stream;
mod index_allocation;
mod mft;
mod mft_stream;
mod path;
mod reader;
mod recovery;
mod utils;
mod write_map;

pub use attribute::{AttributeListEntry, DataAttributeExtent};
pub use data_runs::DataRun;
pub use directory::NtfsDirectoryEntry;
pub use file_stream::NtfsFileReader;
pub use mft::{NtfsPreviewFile, NtfsReader};
pub use recovery::{NtfsAllocationState, NtfsDataExtent, NtfsDeletedFileRecord};
pub use utils::parse_mft_data_real_size;
pub use write_map::NtfsFileExtent;

pub(crate) use evidence_core::filesystem::{
    file_not_found, fs_out_of_memory, invalid_fs_data, truncate_data_to_declared_size,
    unexpected_fs_eof,
};

pub(crate) const ATTR_TYPE_ATTRIBUTE_LIST: u32 = 0x20;
pub(crate) const ATTR_TYPE_DATA: u32 = 0x80;
pub(crate) const ATTR_TYPE_INDEX_ROOT: u32 = 0x90;
pub(crate) const ATTR_TYPE_INDEX_ALLOCATION: u32 = 0xA0;
pub(crate) const ATTR_TYPE_BITMAP: u32 = 0xB0;
pub(crate) const ATTR_TYPE_END: u32 = 0xFFFF_FFFF;
pub(crate) const MAX_EXTERNAL_ATTRIBUTE_RECORDS: usize = 256;
pub(crate) const MAX_ATTRIBUTE_LIST_ENTRIES: usize = 4096;
pub(crate) const MAX_BUFFERED_FILE_BYTES: usize = 256 * 1024 * 1024;
