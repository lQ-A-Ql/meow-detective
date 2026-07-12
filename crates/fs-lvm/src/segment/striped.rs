use crate::error::{LvmError, Result};
use crate::metadata::SegmentMeta;

use super::area::map_area_range;
use super::math::{checked_add, checked_mul};
use super::{LvExtent, MapContext};

pub(super) fn build_striped(
    context: &MapContext<'_>,
    segment: &SegmentMeta,
    base_logical_extent: u64,
    stripe_count: u64,
    stripe_size_sectors: u64,
    stack: &mut Vec<String>,
) -> Result<Vec<LvExtent>> {
    validate_stripe_count(segment, stripe_count)?;
    let stripe_size_bytes =
        checked_mul(stripe_size_sectors, 512, "striped stripe_size byte length")?;
    if stripe_size_bytes == 0 {
        return Err(LvmError::MetadataParseError {
            line: 0,
            message: "striped segment has stripe_size 0".to_string(),
        });
    }

    let logical_start = checked_mul(
        base_logical_extent,
        context.extent_size_bytes,
        "striped logical start",
    )?;
    let segment_len = checked_mul(
        segment.extent_count,
        context.extent_size_bytes,
        "striped segment length",
    )?;
    map_stripe_chunks(
        context,
        segment,
        logical_start,
        segment_len,
        stripe_count,
        stripe_size_bytes,
        stack,
    )
}

fn map_stripe_chunks(
    context: &MapContext<'_>,
    segment: &SegmentMeta,
    logical_start: u64,
    segment_len: u64,
    stripe_count: u64,
    stripe_size_bytes: u64,
    stack: &mut Vec<String>,
) -> Result<Vec<LvExtent>> {
    let mut map = Vec::new();
    let mut segment_offset = 0u64;
    while segment_offset < segment_len {
        let chunk_number = segment_offset / stripe_size_bytes;
        let stripe_index = (chunk_number % stripe_count) as usize;
        let stripe_set = chunk_number / stripe_count;
        let in_chunk_offset = segment_offset % stripe_size_bytes;
        let length = (stripe_size_bytes - in_chunk_offset).min(segment_len - segment_offset);
        let area_offset = checked_add(
            checked_mul(stripe_set, stripe_size_bytes, "striped set byte offset")?,
            in_chunk_offset,
            "striped in-chunk offset",
        )?;
        let chunk_logical_start =
            checked_add(logical_start, segment_offset, "striped logical chunk start")?;
        map.extend(map_area_range(
            context,
            &segment.areas[stripe_index],
            chunk_logical_start,
            area_offset,
            length,
            stack,
        )?);
        segment_offset = checked_add(segment_offset, length, "striped segment cursor")?;
    }
    Ok(map)
}

fn validate_stripe_count(segment: &SegmentMeta, stripe_count: u64) -> Result<()> {
    if stripe_count == 0 {
        return Err(LvmError::MetadataParseError {
            line: 0,
            message: "striped segment has stripe_count 0".to_string(),
        });
    }
    let area_count = if segment.areas.is_empty() {
        segment.stripes.len()
    } else {
        segment.areas.len()
    };
    if area_count == 0 {
        return Err(LvmError::MetadataParseError {
            line: 0,
            message: "striped segment has no data areas".to_string(),
        });
    }
    if area_count != stripe_count as usize {
        return Err(LvmError::MetadataParseError {
            line: 0,
            message: format!(
                "striped segment stripe_count {stripe_count} does not match {area_count} data area entries"
            ),
        });
    }
    Ok(())
}
