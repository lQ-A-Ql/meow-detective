//! LVM2 metadata-area and text-metadata parsing.

mod region;
mod segments;
mod text;
mod types;

pub use region::parse_metadata;
pub(crate) use region::parse_metadata_from_regions;
pub use types::{
    LvMeta, LvRole, PvMeta, RaidComponent, RaidComponentSource, SegmentArea, SegmentDependencies,
    SegmentMeta, SegmentType, VolumeGroup,
};

#[cfg(test)]
#[path = "../tests/unit/metadata.rs"]
mod tests;
