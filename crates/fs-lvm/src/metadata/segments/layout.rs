use crate::error::{LvmError, Result};
use crate::metadata::{SegmentArea, SegmentDependencies, SegmentMeta, SegmentType};

pub(crate) fn unsupported_lv_segment_with_areas_and_dependencies(
    extent_count: u64,
    reason: String,
    areas: Vec<SegmentArea>,
    dependencies: SegmentDependencies,
) -> SegmentMeta {
    SegmentMeta {
        start_extent: 0,
        extent_count,
        seg_type: SegmentType::Unsupported { type_name: reason },
        stripes: Vec::new(),
        areas,
        dependencies,
    }
}

pub(crate) fn merge_segment_dependencies(segments: &[SegmentMeta]) -> SegmentDependencies {
    let mut merged = SegmentDependencies::default();
    for segment in segments {
        merge_string_fields(&mut merged, &segment.dependencies);
        merge_numeric_fields(&mut merged, &segment.dependencies);
        if merged.raid_component_source.is_none() {
            merged.raid_component_source = segment.dependencies.raid_component_source;
        }
        if merged.raid_components.is_empty() {
            merged.raid_components = segment.dependencies.raid_components.clone();
        }
    }
    merged
}

fn merge_string_fields(target: &mut SegmentDependencies, source: &SegmentDependencies) {
    merge_optional_string(&mut target.thin_pool, &source.thin_pool);
    merge_optional_string(&mut target.metadata, &source.metadata);
    merge_optional_string(&mut target.pool, &source.pool);
    merge_optional_string(&mut target.data, &source.data);
    merge_optional_string(&mut target.origin, &source.origin);
    merge_optional_string(&mut target.external_origin, &source.external_origin);
    merge_optional_string(&mut target.cow_store, &source.cow_store);
    merge_optional_string(&mut target.merging_store, &source.merging_store);
    merge_optional_string(&mut target.cache_pool, &source.cache_pool);
    merge_optional_string(&mut target.metadata_id, &source.metadata_id);
    merge_optional_string(&mut target.data_id, &source.data_id);
}

fn merge_numeric_fields(target: &mut SegmentDependencies, source: &SegmentDependencies) {
    target.transaction_id = target.transaction_id.or(source.transaction_id);
    target.device_id = target.device_id.or(source.device_id);
    target.chunk_size = target.chunk_size.or(source.chunk_size);
    target.metadata_format = target.metadata_format.or(source.metadata_format);
    target.metadata_start = target.metadata_start.or(source.metadata_start);
    target.metadata_len = target.metadata_len.or(source.metadata_len);
    target.data_start = target.data_start.or(source.data_start);
    target.data_len = target.data_len.or(source.data_len);
}

fn merge_optional_string(target: &mut Option<String>, source: &Option<String>) {
    if target.is_none() {
        *target = source.clone();
    }
}

pub(crate) fn max_segment_end(segments: &[SegmentMeta]) -> Result<u64> {
    segments.iter().try_fold(0u64, |max_end, segment| {
        let end = segment
            .start_extent
            .checked_add(segment.extent_count)
            .ok_or_else(|| LvmError::MetadataParseError {
                line: 0,
                message: "logical volume extent range overflows u64".to_string(),
            })?;
        Ok(max_end.max(end))
    })
}

pub(crate) fn validate_segment_layout(
    segments: &[SegmentMeta],
    context: &str,
) -> std::result::Result<(), String> {
    if segments.is_empty() {
        return Err(format!("{context} contains no segment blocks"));
    }
    let mut ranges = segments
        .iter()
        .map(|segment| {
            segment
                .start_extent
                .checked_add(segment.extent_count)
                .map(|end| (segment.start_extent, end))
                .ok_or_else(|| format!("{context} segment extent range overflows u64"))
        })
        .collect::<std::result::Result<Vec<_>, _>>()?;
    ranges.sort_by_key(|(start, _)| *start);

    let mut expected_start = 0u64;
    for (start, end) in ranges {
        if start != expected_start {
            let relation = if start > expected_start {
                "gap"
            } else {
                "overlap"
            };
            return Err(format!(
                "{context} has segment {relation}: expected start_extent {expected_start} but found {start}"
            ));
        }
        expected_start = end;
    }
    Ok(())
}
