//! Read-only Android dynamic-partition metadata and logical extent mapping.
//!
//! The crate implements the AOSP liblp wire contract and delegates filesystem
//! opening to the validated ext4, F2FS, and EROFS readers. It does not discover
//! GPT partitions, choose an active A/B slot, or mutate super metadata.

mod bytes;
mod error;
mod filesystem;
mod geometry;
mod metadata;
mod reader;
mod reader_factory;

pub use error::{Result, VolumeAndroidError};
pub use filesystem::{probe_filesystem, AndroidFilesystemKind};
pub use geometry::{GeometryCopy, LpGeometry};
pub use metadata::{
    BlockDevice, LogicalExtent, LogicalExtentTarget, LogicalPartition, MetadataCopy, SuperMetadata,
};
pub use reader::LogicalPartitionReader;
pub use reader_factory::open_filesystem_reader;

pub const LP_SECTOR_SIZE: u64 = 512;
pub const LP_METADATA_GEOMETRY_MAGIC: u32 = 0x616c_4467;
pub const LP_METADATA_HEADER_MAGIC: u32 = 0x414c_5030;
