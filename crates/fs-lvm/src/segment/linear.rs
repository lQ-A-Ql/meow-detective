use crate::error::{LvmError, Result};
use crate::metadata::SegmentMeta;

use super::area::map_area_range;
use super::math::checked_mul;
use super::{LvExtent, MapContext};

pub(super) fn build_linear(
    context: &MapContext<'_>,
    segment: &SegmentMeta,
    base_logical_extent: u64,
    stack: &mut Vec<String>,
) -> Result<Vec<LvExtent>> {
    if segment.areas.len() != 1 {
        let message = if segment.areas.is_empty() {
            "linear segment has no data areas".to_string()
        } else {
            format!(
                "linear segment expected 1 data area but found {}",
                segment.areas.len()
            )
        };
        return Err(LvmError::MetadataParseError { line: 0, message });
    }

    let logical_start = checked_mul(
        base_logical_extent,
        context.extent_size_bytes,
        "linear logical start",
    )?;
    let length = checked_mul(
        segment.extent_count,
        context.extent_size_bytes,
        "linear segment length",
    )?;
    map_area_range(context, &segment.areas[0], logical_start, 0, length, stack)
}
