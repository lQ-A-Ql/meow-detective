use std::collections::HashSet;

use crate::error::{LvmError, Result};

use super::areas::{
    parse_optional_areas, parse_raid_component_areas, parse_raid_component_list,
    parse_required_stripes,
};
use super::{ParsedSegmentParts, SegmentParseError, UnsupportedSegmentDetails};
use crate::metadata::text::{optional_string, optional_u64, required_string, required_u64};
use crate::metadata::{
    RaidComponentSource, SegmentArea, SegmentDependencies, SegmentMeta, SegmentType,
};

pub(super) fn parse_raid_segment(
    seg_type: SegmentType,
    params: &[(String, String)],
    start_extent: u64,
    extent_count: u64,
    lv_names: &HashSet<&str>,
) -> std::result::Result<ParsedSegmentParts, SegmentParseError> {
    let component_source = raid_component_source(&seg_type)?;
    let component_key = raid_component_key(component_source);

    if let Ok((areas, components)) =
        parse_raid_component_areas(params, component_key, component_source, lv_names)
    {
        let dependencies = SegmentDependencies {
            raid_component_source: Some(component_source),
            raid_components: components,
            ..SegmentDependencies::default()
        };
        return Err(SegmentParseError::unsupported(
            unsupported_segment_with_areas_and_dependencies(
                start_extent,
                extent_count,
                format!(
                    "{} requires RAID component LV graph reconstruction",
                    raid_segment_label(&seg_type)
                ),
                areas,
                dependencies,
            ),
            format!(
                "{} uses LVM component LV list and is not directly mappable",
                raid_segment_label(&seg_type)
            ),
        ));
    }

    let reason = match required_string(params, component_key, "raid component list") {
        Err(error) => lvm_error_message(&error),
        Ok(raw) => match parse_raid_component_list(&raw, component_source, lv_names) {
            Err(error) => lvm_error_message(&error),
            Ok(_) => "raid component list is not directly mappable".to_string(),
        },
    };
    Err(SegmentParseError::unsupported(
        unsupported_segment(start_extent, extent_count, reason.clone()),
        reason,
    ))
}

pub(super) fn unsupported_raid_or_mirror_segment(
    params: &[(String, String)],
    context: &str,
    start_extent: u64,
    extent_count: u64,
    reason: &str,
    pv_names: &HashSet<&str>,
    lv_names: &HashSet<&str>,
) -> std::result::Result<UnsupportedSegmentDetails, SegmentParseError> {
    if let Some(stripe_count) = optional_u64(params, "stripe_count") {
        let _ = parse_required_stripes(params, context, stripe_count);
    }
    let mut dependencies = SegmentDependencies::default();
    let mut areas = match parse_optional_areas(params, pv_names, lv_names) {
        Ok(areas) if !areas.is_empty() => areas,
        _ => Vec::new(),
    };
    if areas.is_empty() {
        if let Ok((component_areas, components)) =
            parse_raid_component_areas(params, "raids", RaidComponentSource::Raids, lv_names)
        {
            areas = component_areas;
            dependencies.raid_component_source = Some(RaidComponentSource::Raids);
            dependencies.raid_components = components;
        }
    }
    Ok(UnsupportedSegmentDetails {
        segment: unsupported_segment_with_areas_and_dependencies(
            start_extent,
            extent_count,
            reason.to_string(),
            areas,
            dependencies,
        ),
    })
}

pub(super) fn parse_thin_segment(
    params: &[(String, String)],
    context: &str,
    lv_names: &HashSet<&str>,
) -> Result<ParsedSegmentParts> {
    let mut dependencies = SegmentDependencies {
        thin_pool: Some(required_lv_ref(params, "thin_pool", context, lv_names)?),
        transaction_id: Some(required_u64(params, "transaction_id", context)?),
        device_id: Some(required_u64(params, "device_id", context)?),
        ..SegmentDependencies::default()
    };
    dependencies.origin = optional_lv_ref(params, "origin", lv_names)?;
    dependencies.external_origin = optional_lv_ref(params, "external_origin", lv_names)?;
    let areas = dependencies_to_areas(&dependencies);
    Ok(ParsedSegmentParts {
        seg_type: SegmentType::ThinVolume,
        stripes: Vec::new(),
        areas,
        dependencies,
    })
}

pub(super) fn parse_thin_pool_segment(
    params: &[(String, String)],
    context: &str,
    lv_names: &HashSet<&str>,
) -> Result<ParsedSegmentParts> {
    let dependencies = SegmentDependencies {
        metadata: Some(required_lv_ref(params, "metadata", context, lv_names)?),
        pool: Some(required_lv_ref(params, "pool", context, lv_names)?),
        transaction_id: Some(required_u64(params, "transaction_id", context)?),
        chunk_size: Some(required_u64(params, "chunk_size", context)?),
        ..SegmentDependencies::default()
    };
    let areas = dependencies_to_areas(&dependencies);
    Ok(ParsedSegmentParts {
        seg_type: SegmentType::ThinPool,
        stripes: Vec::new(),
        areas,
        dependencies,
    })
}

pub(super) fn parse_cache_segment(
    params: &[(String, String)],
    context: &str,
    lv_names: &HashSet<&str>,
) -> Result<ParsedSegmentParts> {
    let dependencies = SegmentDependencies {
        cache_pool: Some(required_lv_ref(params, "cache_pool", context, lv_names)?),
        origin: Some(required_lv_ref(params, "origin", context, lv_names)?),
        chunk_size: optional_u64(params, "chunk_size"),
        metadata_format: optional_u64(params, "metadata_format"),
        metadata_start: optional_u64(params, "metadata_start"),
        metadata_len: optional_u64(params, "metadata_len"),
        data_start: optional_u64(params, "data_start"),
        data_len: optional_u64(params, "data_len"),
        metadata_id: optional_string(params, "metadata_id"),
        data_id: optional_string(params, "data_id"),
        ..SegmentDependencies::default()
    };
    let areas = dependencies_to_areas(&dependencies);
    Ok(ParsedSegmentParts {
        seg_type: SegmentType::CacheVolume,
        stripes: Vec::new(),
        areas,
        dependencies,
    })
}

pub(super) fn parse_cache_pool_segment(
    params: &[(String, String)],
    context: &str,
    lv_names: &HashSet<&str>,
) -> Result<ParsedSegmentParts> {
    let data_key = if params.iter().any(|(key, _)| key == "data") {
        "data"
    } else {
        "pool"
    };
    let dependencies = SegmentDependencies {
        data: Some(required_lv_ref(params, data_key, context, lv_names)?),
        metadata: Some(required_lv_ref(params, "metadata", context, lv_names)?),
        chunk_size: optional_u64(params, "chunk_size"),
        metadata_format: optional_u64(params, "metadata_format"),
        ..SegmentDependencies::default()
    };
    let areas = dependencies_to_areas(&dependencies);
    Ok(ParsedSegmentParts {
        seg_type: SegmentType::CachePool,
        stripes: Vec::new(),
        areas,
        dependencies,
    })
}

pub(super) fn parse_snapshot_segment(
    params: &[(String, String)],
    context: &str,
    lv_names: &HashSet<&str>,
) -> Result<ParsedSegmentParts> {
    let mut dependencies = SegmentDependencies {
        origin: Some(required_lv_ref(params, "origin", context, lv_names)?),
        chunk_size: Some(required_u64(params, "chunk_size", context)?),
        ..SegmentDependencies::default()
    };
    dependencies.cow_store = optional_lv_ref(params, "cow_store", lv_names)?;
    dependencies.merging_store = optional_lv_ref(params, "merging_store", lv_names)?;
    if dependencies.cow_store.is_none() && dependencies.merging_store.is_none() {
        return Err(LvmError::MetadataParseError {
            line: 0,
            message: format!("snapshot segment missing cow_store or merging_store in {context}"),
        });
    }
    let areas = dependencies_to_areas(&dependencies);
    Ok(ParsedSegmentParts {
        seg_type: SegmentType::Snapshot,
        stripes: Vec::new(),
        areas,
        dependencies,
    })
}

pub(super) fn unsupported_segment(
    start_extent: u64,
    extent_count: u64,
    reason: String,
) -> SegmentMeta {
    unsupported_segment_with_areas_and_dependencies(
        start_extent,
        extent_count,
        reason,
        Vec::new(),
        SegmentDependencies::default(),
    )
}

pub(super) fn unsupported_segment_with_areas_and_dependencies(
    start_extent: u64,
    extent_count: u64,
    reason: String,
    areas: Vec<SegmentArea>,
    dependencies: SegmentDependencies,
) -> SegmentMeta {
    SegmentMeta {
        start_extent,
        extent_count,
        seg_type: SegmentType::Unsupported { type_name: reason },
        stripes: Vec::new(),
        areas,
        dependencies,
    }
}

fn raid_component_source(
    segment_type: &SegmentType,
) -> std::result::Result<RaidComponentSource, SegmentParseError> {
    match segment_type {
        SegmentType::Raid0 { .. } => Ok(RaidComponentSource::Raid0Lvs),
        SegmentType::Raid1 { .. }
        | SegmentType::Raid5 { .. }
        | SegmentType::Raid6 { .. }
        | SegmentType::Raid10 { .. } => Ok(RaidComponentSource::Raids),
        _ => Err(SegmentParseError::Fatal(LvmError::MetadataParseError {
            line: 0,
            message: "segment is not a RAID segment".to_string(),
        })),
    }
}

fn raid_component_key(source: RaidComponentSource) -> &'static str {
    match source {
        RaidComponentSource::Raid0Lvs => "raid0_lvs",
        RaidComponentSource::Raids => "raids",
        RaidComponentSource::Stripes => "stripes",
    }
}

fn raid_segment_label(segment_type: &SegmentType) -> &'static str {
    match segment_type {
        SegmentType::Raid0 { .. } => "raid0",
        SegmentType::Raid1 { .. } => "raid1",
        SegmentType::Raid5 { .. } => "raid5",
        SegmentType::Raid6 { .. } => "raid6",
        SegmentType::Raid10 { .. } => "raid10",
        _ => "raid",
    }
}

fn required_lv_ref(
    params: &[(String, String)],
    key: &str,
    context: &str,
    lv_names: &HashSet<&str>,
) -> Result<String> {
    let name = required_string(params, key, context)?;
    let _known = lv_names.contains(name.as_str());
    Ok(name)
}

fn optional_lv_ref(
    params: &[(String, String)],
    key: &str,
    lv_names: &HashSet<&str>,
) -> Result<Option<String>> {
    let Some((_, name)) = params.iter().find(|(param_key, _)| param_key == key) else {
        return Ok(None);
    };
    let _known = lv_names.contains(name.as_str());
    Ok(Some(name.clone()))
}

fn dependencies_to_areas(dependencies: &SegmentDependencies) -> Vec<SegmentArea> {
    dependencies
        .referenced_lvs()
        .into_iter()
        .map(|name| SegmentArea::LogicalVolume {
            name: name.to_string(),
            start_extent: 0,
        })
        .collect()
}

fn lvm_error_message(error: &LvmError) -> String {
    match error {
        LvmError::MetadataParseError { message, .. } => message.clone(),
        other => other.to_string(),
    }
}
