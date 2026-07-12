//! Read-only LVM2 discovery and logical-volume readers.

pub mod crc;
pub mod error;
pub mod label;
pub mod lv_reader;
pub mod metadata;
mod pool;
mod pool_thin;
pub mod segment;
pub mod thin;
pub mod thin_reader;

pub use error::LvmError;
pub use label::LvmLabel;
pub use lv_reader::LvReader;
pub use metadata::{LvMeta, PvMeta, SegmentArea, SegmentMeta, SegmentType, VolumeGroup};
pub use pool::{LvInfo, LvmPool};
pub use segment::LvExtent;
pub use thin_reader::ThinLvReader;

pub(crate) use pool::lv_info_from_meta;

use error::Result;

pub fn probe_lvm<R>(reader: &mut R, pv_offset: u64) -> Result<bool>
where
    R: std::io::Read + std::io::Seek + ?Sized,
{
    match label::parse_pv_label(reader, pv_offset) {
        Ok(_) => Ok(true),
        Err(LvmError::NotLvm) => Ok(false),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
#[path = "../tests/unit/lib.rs"]
mod tests;
