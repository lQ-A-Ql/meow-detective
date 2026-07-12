use crate::error::{LvmError, Result};
use crate::metadata::SegmentArea;

use super::math::{checked_add, checked_mul};
use super::{build_extent_map_inner, LvExtent, MapContext};

pub(super) fn map_area_range(
    context: &MapContext<'_>,
    area: &SegmentArea,
    outer_logical_start: u64,
    area_relative_offset: u64,
    length: u64,
    stack: &mut Vec<String>,
) -> Result<Vec<LvExtent>> {
    match area {
        SegmentArea::PhysicalVolume { name, start_extent } => map_physical_volume_area(
            context,
            name,
            *start_extent,
            outer_logical_start,
            area_relative_offset,
            length,
        ),
        SegmentArea::LogicalVolume { name, start_extent } => map_logical_volume_area(
            context,
            name,
            *start_extent,
            outer_logical_start,
            area_relative_offset,
            length,
            stack,
        ),
        SegmentArea::Unassigned { .. } => Err(LvmError::UnsupportedSegment {
            lv_name: stack
                .last()
                .cloned()
                .unwrap_or_else(|| "<unknown>".to_string()),
            seg_type: "unassigned segment area".to_string(),
        }),
    }
}

fn map_physical_volume_area(
    context: &MapContext<'_>,
    name: &str,
    start_extent: u64,
    logical_start: u64,
    area_relative_offset: u64,
    length: u64,
) -> Result<Vec<LvExtent>> {
    let pv_index = find_pv_index(context.pv_data_offsets, name)?;
    let pv_data_start = context.pv_data_offsets[pv_index].1;
    let area_start = checked_mul(start_extent, context.extent_size_bytes, "PV area start")?;
    let physical_offset = checked_add(
        checked_add(pv_data_start, area_start, "PV area physical start")?,
        area_relative_offset,
        "PV area relative offset",
    )?;
    Ok(vec![LvExtent {
        logical_start,
        physical_offset,
        length,
        pv_index,
    }])
}

fn map_logical_volume_area(
    context: &MapContext<'_>,
    name: &str,
    start_extent: u64,
    outer_logical_start: u64,
    area_relative_offset: u64,
    length: u64,
    stack: &mut Vec<String>,
) -> Result<Vec<LvExtent>> {
    reject_cycle(stack, name)?;
    let target = context
        .vg
        .logical_volumes
        .iter()
        .find(|volume| volume.name == name)
        .ok_or_else(|| LvmError::MetadataParseError {
            line: 0,
            message: format!("unknown logical volume '{name}' referenced in segment mapping"),
        })?;
    stack.push(name.to_string());
    let target_map = build_extent_map_inner(context, target, stack);
    stack.pop();
    let target_map = target_map?;
    let source_start = checked_add(
        checked_mul(
            start_extent,
            context.extent_size_bytes,
            "logical-volume area start",
        )?,
        area_relative_offset,
        "logical-volume area relative offset",
    )?;
    slice_extent_map(&target_map, source_start, length, outer_logical_start, name)
}

fn reject_cycle(stack: &[String], name: &str) -> Result<()> {
    if !stack.iter().any(|entry| entry == name) {
        return Ok(());
    }
    let mut cycle = stack.join(" -> ");
    cycle.push_str(" -> ");
    cycle.push_str(name);
    Err(LvmError::MetadataParseError {
        line: 0,
        message: format!("cyclic LVM logical-volume area reference: {cycle}"),
    })
}

fn slice_extent_map(
    extents: &[LvExtent],
    source_start: u64,
    length: u64,
    outer_logical_start: u64,
    source_lv_name: &str,
) -> Result<Vec<LvExtent>> {
    let source_end = checked_add(source_start, length, "logical-volume area end")?;
    let mut cursor = source_start;
    let mut sliced = Vec::new();
    for extent in extents {
        if let Some(mapped) = slice_extent(
            extent,
            cursor,
            source_start,
            source_end,
            outer_logical_start,
            source_lv_name,
        )? {
            cursor = mapped.logical_end;
            sliced.push(mapped.extent);
            if cursor == source_end {
                break;
            }
        }
    }
    if cursor != source_end {
        return Err(LvmError::MetadataParseError {
            line: 0,
            message: format!(
                "logical volume '{source_lv_name}' area ended before requested byte {source_end}"
            ),
        });
    }
    Ok(sliced)
}

struct SlicedExtent {
    extent: LvExtent,
    logical_end: u64,
}

fn slice_extent(
    extent: &LvExtent,
    cursor: u64,
    source_start: u64,
    source_end: u64,
    outer_logical_start: u64,
    source_lv_name: &str,
) -> Result<Option<SlicedExtent>> {
    let extent_end = checked_add(extent.logical_start, extent.length, "source extent end")?;
    if extent_end <= cursor || extent.logical_start >= source_end {
        return Ok(None);
    }
    if extent.logical_start > cursor {
        return Err(LvmError::MetadataParseError {
            line: 0,
            message: format!(
                "logical volume '{source_lv_name}' area has uncovered logical range at byte {cursor}"
            ),
        });
    }
    let overlap_start = cursor.max(extent.logical_start);
    let overlap_end = source_end.min(extent_end);
    if overlap_end <= overlap_start {
        return Ok(None);
    }
    let offset_in_extent = overlap_start - extent.logical_start;
    let offset_in_area = overlap_start - source_start;
    Ok(Some(SlicedExtent {
        extent: LvExtent {
            logical_start: checked_add(
                outer_logical_start,
                offset_in_area,
                "sliced logical extent start",
            )?,
            physical_offset: checked_add(
                extent.physical_offset,
                offset_in_extent,
                "sliced physical extent start",
            )?,
            length: overlap_end - overlap_start,
            pv_index: extent.pv_index,
        },
        logical_end: overlap_end,
    }))
}

fn find_pv_index(pv_data_offsets: &[(String, u64)], pv_name: &str) -> Result<usize> {
    pv_data_offsets
        .iter()
        .position(|(name, _)| name == pv_name)
        .ok_or_else(|| LvmError::UnknownPhysicalVolume {
            name: pv_name.to_string(),
        })
}
