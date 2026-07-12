//! Logical-to-physical extent mapping for LVM logical volumes.

mod area;
mod linear;
mod math;
mod striped;

use crate::error::{LvmError, Result};
use crate::metadata::{LvMeta, SegmentType, VolumeGroup};
use linear::build_linear;
use striped::build_striped;

/// A resolved contiguous byte range on one physical volume.
#[derive(Debug, Clone)]
pub struct LvExtent {
    pub logical_start: u64,
    pub physical_offset: u64,
    pub length: u64,
    pub pv_index: usize,
}

pub(super) struct MapContext<'a> {
    pub(super) vg: &'a VolumeGroup,
    pub(super) pv_data_offsets: &'a [(String, u64)],
    pub(super) extent_size_bytes: u64,
}

/// Build the complete extent map for a logical volume.
pub fn build_extent_map(
    volume_group: &VolumeGroup,
    logical_volume: &LvMeta,
    pv_data_offsets: &[(String, u64)],
) -> Result<Vec<LvExtent>> {
    let extent_size_bytes =
        volume_group
            .extent_size
            .checked_mul(512)
            .ok_or_else(|| LvmError::MetadataParseError {
                line: 0,
                message: format!("extent size overflows bytes for VG '{}'", volume_group.name),
            })?;
    let context = MapContext {
        vg: volume_group,
        pv_data_offsets,
        extent_size_bytes,
    };
    let mut stack = vec![logical_volume.name.clone()];
    build_extent_map_inner(&context, logical_volume, &mut stack)
}

pub(super) fn build_extent_map_inner(
    context: &MapContext<'_>,
    logical_volume: &LvMeta,
    stack: &mut Vec<String>,
) -> Result<Vec<LvExtent>> {
    let mut map = Vec::new();
    for segment in &logical_volume.segments {
        let extents = match &segment.seg_type {
            SegmentType::Linear => build_linear(context, segment, segment.start_extent, stack)?,
            SegmentType::Striped {
                stripe_count,
                stripe_size,
            } => build_striped(
                context,
                segment,
                segment.start_extent,
                *stripe_count,
                *stripe_size,
                stack,
            )?,
            segment_type => {
                return Err(unsupported_segment_error(
                    logical_volume,
                    segment_type,
                    stack,
                ))
            }
        };
        map.extend(extents);
    }
    map.sort_by_key(|extent| extent.logical_start);
    Ok(map)
}

fn unsupported_segment_error(
    logical_volume: &LvMeta,
    segment_type: &SegmentType,
    stack: &[String],
) -> LvmError {
    let description = match segment_type {
        SegmentType::Raid0 { .. } => {
            "raid0 (requires LVM2 raid0_lvs/raids component LV mapping)".to_string()
        }
        SegmentType::Raid1 { .. } | SegmentType::Raid10 { .. } => {
            "raid1/raid10 (requires LVM component LV graph mapping)".to_string()
        }
        SegmentType::Raid5 { .. } | SegmentType::Raid6 { .. } => {
            "raid5/raid6 (parity RAID requires reconstruction logic)".to_string()
        }
        SegmentType::ThinVolume => format!("thin (dependency chain: {})", stack.join(" -> ")),
        SegmentType::ThinPool => format!("thin-pool (dependency chain: {})", stack.join(" -> ")),
        SegmentType::Snapshot => format!("snapshot (dependency chain: {})", stack.join(" -> ")),
        SegmentType::CacheVolume => format!("cache (dependency chain: {})", stack.join(" -> ")),
        SegmentType::CachePool => {
            format!("cache-pool (dependency chain: {})", stack.join(" -> "))
        }
        SegmentType::Unsupported { type_name } => {
            format!("{type_name} (dependency chain: {})", stack.join(" -> "))
        }
        SegmentType::Linear | SegmentType::Striped { .. } => {
            "unexpected directly mappable segment".to_string()
        }
    };
    LvmError::UnsupportedSegment {
        lv_name: logical_volume.name.clone(),
        seg_type: description,
    }
}

#[cfg(test)]
#[path = "../tests/unit/segment.rs"]
mod tests;
