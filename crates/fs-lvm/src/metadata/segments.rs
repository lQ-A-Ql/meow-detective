use std::collections::HashSet;

use crate::error::LvmError;

mod advanced;
mod areas;
mod layout;

use advanced::{
    parse_cache_pool_segment, parse_cache_segment, parse_raid_segment, parse_snapshot_segment,
    parse_thin_pool_segment, parse_thin_segment, unsupported_raid_or_mirror_segment,
    unsupported_segment,
};
pub(crate) use areas::unsupported_segment_type_name;
use areas::{
    parse_linear_areas, parse_optional_areas, parse_required_stripes, required_stripe_size,
    resolve_stripe_areas, stripes_from_pv_areas,
};
pub(crate) use layout::{
    max_segment_end, merge_segment_dependencies,
    unsupported_lv_segment_with_areas_and_dependencies, validate_segment_layout,
};

use super::text::{required_string, required_u64, SegmentRaw};
use super::{SegmentArea, SegmentDependencies, SegmentMeta, SegmentType};

pub(super) enum SegmentParseError {
    Unsupported {
        segment: Box<SegmentMeta>,
        reason: String,
    },
    Fatal(LvmError),
}

pub(super) struct UnsupportedSegmentDetails {
    pub(super) segment: SegmentMeta,
}

pub(super) struct ParsedSegmentParts {
    pub(super) seg_type: SegmentType,
    pub(super) stripes: Vec<(String, u64)>,
    pub(super) areas: Vec<SegmentArea>,
    pub(super) dependencies: SegmentDependencies,
}

impl SegmentParseError {
    pub(super) fn fatal(message: String) -> Self {
        SegmentParseError::Fatal(LvmError::MetadataParseError { line: 0, message })
    }

    pub(super) fn unsupported(segment: SegmentMeta, reason: String) -> Self {
        SegmentParseError::Unsupported {
            segment: Box::new(segment),
            reason,
        }
    }
}

pub(super) fn parse_segment(
    segment: &SegmentRaw,
    lv_context: &str,
    pv_names: &HashSet<&str>,
    lv_names: &HashSet<&str>,
) -> std::result::Result<SegmentMeta, SegmentParseError> {
    let context = format!("{lv_context} segment '{}'", segment.name);
    let start_extent = required_u64(&segment.params, "start_extent", &context)
        .map_err(SegmentParseError::Fatal)?;
    let extent_count = required_u64(&segment.params, "extent_count", &context)
        .map_err(SegmentParseError::Fatal)?;
    if extent_count == 0 {
        return Err(SegmentParseError::fatal(format!(
            "extent_count must be greater than zero in {context}"
        )));
    }
    let type_name =
        required_string(&segment.params, "type", &context).map_err(SegmentParseError::Fatal)?;
    let parts = parse_segment_parts(
        &type_name,
        &segment.params,
        &context,
        start_extent,
        extent_count,
        pv_names,
        lv_names,
    )?;
    let metadata = SegmentMeta {
        start_extent,
        extent_count,
        seg_type: parts.seg_type,
        stripes: parts.stripes,
        areas: parts.areas,
        dependencies: parts.dependencies,
    };
    if let Some(unsupported_type) = unsupported_segment_type_name(&metadata) {
        let reason = unsupported_type.to_string();
        return Err(SegmentParseError::unsupported(metadata, reason));
    }
    Ok(metadata)
}

fn parse_segment_parts(
    type_name: &str,
    params: &[(String, String)],
    context: &str,
    start_extent: u64,
    extent_count: u64,
    pv_names: &HashSet<&str>,
    lv_names: &HashSet<&str>,
) -> std::result::Result<ParsedSegmentParts, SegmentParseError> {
    match type_name {
        "linear" => parse_linear_segment(params, context, pv_names, lv_names),
        "striped" => parse_striped_segment(
            params,
            context,
            start_extent,
            extent_count,
            pv_names,
            lv_names,
        ),
        "raid0" => parse_raid_segment(
            SegmentType::Raid0 {
                stripe_count: required_u64(params, "stripe_count", context)
                    .map_err(SegmentParseError::Fatal)?,
                stripe_size: required_stripe_size(params, context)
                    .map_err(SegmentParseError::Fatal)?,
            },
            params,
            start_extent,
            extent_count,
            lv_names,
        ),
        "raid1" | "mirror" => parse_unsupported_mirror(
            params,
            context,
            start_extent,
            extent_count,
            "raid1/mirror requires LVM component LV mapping and sync-state validation",
            pv_names,
            lv_names,
        ),
        "raid10" => parse_unsupported_mirror(
            params,
            context,
            start_extent,
            extent_count,
            "raid10 requires LVM component LV mapping and stripe/mirror reconstruction",
            pv_names,
            lv_names,
        ),
        "raid5" => parse_raid_segment(
            SegmentType::Raid5 {
                stripe_count: required_u64(params, "stripe_count", context)
                    .map_err(SegmentParseError::Fatal)?,
            },
            params,
            start_extent,
            extent_count,
            lv_names,
        ),
        "raid6" => parse_raid_segment(
            SegmentType::Raid6 {
                stripe_count: required_u64(params, "stripe_count", context)
                    .map_err(SegmentParseError::Fatal)?,
            },
            params,
            start_extent,
            extent_count,
            lv_names,
        ),
        "thin" => parse_thin_segment(params, context, lv_names).map_err(SegmentParseError::Fatal),
        "thin-pool" => {
            parse_thin_pool_segment(params, context, lv_names).map_err(SegmentParseError::Fatal)
        }
        "snapshot" => {
            parse_snapshot_segment(params, context, lv_names).map_err(SegmentParseError::Fatal)
        }
        "cache" => parse_cache_segment(params, context, lv_names).map_err(SegmentParseError::Fatal),
        "cache-pool" => {
            parse_cache_pool_segment(params, context, lv_names).map_err(SegmentParseError::Fatal)
        }
        other => Ok(ParsedSegmentParts {
            seg_type: SegmentType::Unsupported {
                type_name: other.to_string(),
            },
            stripes: Vec::new(),
            areas: parse_optional_areas(params, pv_names, lv_names).unwrap_or_default(),
            dependencies: SegmentDependencies::default(),
        }),
    }
}

fn parse_linear_segment(
    params: &[(String, String)],
    context: &str,
    pv_names: &HashSet<&str>,
    lv_names: &HashSet<&str>,
) -> std::result::Result<ParsedSegmentParts, SegmentParseError> {
    let (stripes, areas) = parse_linear_areas(params, context, pv_names, lv_names)
        .map_err(SegmentParseError::Fatal)?;
    Ok(ParsedSegmentParts {
        seg_type: SegmentType::Linear,
        stripes,
        areas,
        dependencies: SegmentDependencies::default(),
    })
}

fn parse_striped_segment(
    params: &[(String, String)],
    context: &str,
    start_extent: u64,
    extent_count: u64,
    pv_names: &HashSet<&str>,
    lv_names: &HashSet<&str>,
) -> std::result::Result<ParsedSegmentParts, SegmentParseError> {
    let stripe_count =
        required_u64(params, "stripe_count", context).map_err(SegmentParseError::Fatal)?;
    if stripe_count == 0 {
        return Err(SegmentParseError::fatal(format!(
            "stripe_count must be greater than zero in {context}"
        )));
    }
    let stripes =
        parse_required_stripes(params, context, stripe_count).map_err(SegmentParseError::Fatal)?;
    let areas =
        resolve_stripe_areas(&stripes, pv_names, lv_names).map_err(SegmentParseError::Fatal)?;
    if stripe_count == 1 {
        return Ok(ParsedSegmentParts {
            seg_type: SegmentType::Linear,
            stripes: stripes_from_pv_areas(&areas),
            areas,
            dependencies: SegmentDependencies::default(),
        });
    }
    let stripe_size = match required_stripe_size(params, context) {
        Ok(size) => size,
        Err(error) => {
            let reason = metadata_error_message(&error);
            return Err(SegmentParseError::unsupported(
                unsupported_segment(start_extent, extent_count, reason.clone()),
                reason,
            ));
        }
    };
    Ok(ParsedSegmentParts {
        seg_type: SegmentType::Striped {
            stripe_count,
            stripe_size,
        },
        stripes: stripes_from_pv_areas(&areas),
        areas,
        dependencies: SegmentDependencies::default(),
    })
}

fn parse_unsupported_mirror(
    params: &[(String, String)],
    context: &str,
    start_extent: u64,
    extent_count: u64,
    reason: &str,
    pv_names: &HashSet<&str>,
    lv_names: &HashSet<&str>,
) -> std::result::Result<ParsedSegmentParts, SegmentParseError> {
    let details = unsupported_raid_or_mirror_segment(
        params,
        context,
        start_extent,
        extent_count,
        reason,
        pv_names,
        lv_names,
    )?;
    Ok(ParsedSegmentParts {
        seg_type: details.segment.seg_type,
        stripes: details.segment.stripes,
        areas: details.segment.areas,
        dependencies: details.segment.dependencies,
    })
}

fn metadata_error_message(error: &LvmError) -> String {
    match error {
        LvmError::MetadataParseError { message, .. } => message.clone(),
        other => other.to_string(),
    }
}
