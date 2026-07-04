use std::collections::HashSet;

use crate::error::{LvmError, Result};

use super::{
    optional_string, optional_u64, required_string, required_u64, RaidComponent,
    RaidComponentSource, SegmentArea, SegmentDependencies, SegmentMeta, SegmentRaw, SegmentType,
};

pub(super) enum SegmentParseError {
    Unsupported {
        segment: Box<SegmentMeta>,
        reason: String,
    },
    Fatal(LvmError),
}

struct UnsupportedSegmentDetails {
    segment: SegmentMeta,
}

struct ParsedSegmentParts {
    seg_type: SegmentType,
    stripes: Vec<(String, u64)>,
    areas: Vec<SegmentArea>,
    dependencies: SegmentDependencies,
}

type StripeAreas = (Vec<(String, u64)>, Vec<SegmentArea>);

impl SegmentParseError {
    fn fatal(message: String) -> Self {
        SegmentParseError::Fatal(LvmError::MetadataParseError { line: 0, message })
    }

    fn unsupported(segment: SegmentMeta, reason: String) -> Self {
        SegmentParseError::Unsupported {
            segment: Box::new(segment),
            reason,
        }
    }
}

pub(super) fn parse_segment(
    seg: &SegmentRaw,
    lv_context: &str,
    pv_names: &HashSet<&str>,
    lv_names: &HashSet<&str>,
) -> std::result::Result<SegmentMeta, SegmentParseError> {
    let context = format!("{} segment '{}'", lv_context, seg.name);
    let start_extent =
        required_u64(&seg.params, "start_extent", &context).map_err(SegmentParseError::Fatal)?;
    let extent_count =
        required_u64(&seg.params, "extent_count", &context).map_err(SegmentParseError::Fatal)?;
    if extent_count == 0 {
        return Err(SegmentParseError::fatal(format!(
            "extent_count must be greater than zero in {}",
            context
        )));
    }
    let type_name =
        required_string(&seg.params, "type", &context).map_err(SegmentParseError::Fatal)?;

    let ParsedSegmentParts {
        seg_type,
        stripes,
        areas,
        dependencies,
    } = match type_name.as_str() {
        "linear" => {
            let (stripes, areas) = parse_linear_areas(&seg.params, &context, pv_names, lv_names)
                .map_err(SegmentParseError::Fatal)?;
            ParsedSegmentParts {
                seg_type: SegmentType::Linear,
                stripes,
                areas,
                dependencies: SegmentDependencies::default(),
            }
        }
        "striped" => {
            let stripe_count = required_u64(&seg.params, "stripe_count", &context)
                .map_err(SegmentParseError::Fatal)?;
            if stripe_count == 0 {
                return Err(SegmentParseError::fatal(format!(
                    "stripe_count must be greater than zero in {}",
                    context
                )));
            }
            let stripes = parse_required_stripes(&seg.params, &context, stripe_count)
                .map_err(SegmentParseError::Fatal)?;
            let areas = resolve_stripe_areas(&stripes, pv_names, lv_names)
                .map_err(SegmentParseError::Fatal)?;
            if stripe_count == 1 {
                ParsedSegmentParts {
                    seg_type: SegmentType::Linear,
                    stripes: stripes_from_pv_areas(&areas),
                    areas,
                    dependencies: SegmentDependencies::default(),
                }
            } else {
                let stripe_size = match required_stripe_size(&seg.params, &context) {
                    Ok(stripe_size) => stripe_size,
                    Err(err) => {
                        let reason = lvm_error_message(&err);
                        return Err(SegmentParseError::unsupported(
                            unsupported_segment(start_extent, extent_count, reason.clone()),
                            reason,
                        ));
                    }
                };
                ParsedSegmentParts {
                    seg_type: SegmentType::Striped {
                        stripe_count,
                        stripe_size,
                    },
                    stripes: stripes_from_pv_areas(&areas),
                    areas,
                    dependencies: SegmentDependencies::default(),
                }
            }
        }
        "raid0" => parse_raid_segment(
            SegmentType::Raid0 {
                stripe_count: required_u64(&seg.params, "stripe_count", &context)
                    .map_err(SegmentParseError::Fatal)?,
                stripe_size: required_stripe_size(&seg.params, &context)
                    .map_err(SegmentParseError::Fatal)?,
            },
            &seg.params,
            &context,
            start_extent,
            extent_count,
            pv_names,
            lv_names,
        )?,
        "raid1" | "mirror" => {
            let details = unsupported_raid_or_mirror_segment(
                &seg.params,
                &context,
                start_extent,
                extent_count,
                "raid1/mirror requires LVM component LV mapping and sync-state validation",
                pv_names,
                lv_names,
            )?;
            ParsedSegmentParts {
                seg_type: details.segment.seg_type,
                stripes: details.segment.stripes,
                areas: details.segment.areas,
                dependencies: details.segment.dependencies,
            }
        }
        "raid10" => {
            let details = unsupported_raid_or_mirror_segment(
                &seg.params,
                &context,
                start_extent,
                extent_count,
                "raid10 requires LVM component LV mapping and stripe/mirror reconstruction",
                pv_names,
                lv_names,
            )?;
            ParsedSegmentParts {
                seg_type: details.segment.seg_type,
                stripes: details.segment.stripes,
                areas: details.segment.areas,
                dependencies: details.segment.dependencies,
            }
        }
        "raid5" => parse_raid_segment(
            SegmentType::Raid5 {
                stripe_count: required_u64(&seg.params, "stripe_count", &context)
                    .map_err(SegmentParseError::Fatal)?,
            },
            &seg.params,
            &context,
            start_extent,
            extent_count,
            pv_names,
            lv_names,
        )?,
        "raid6" => parse_raid_segment(
            SegmentType::Raid6 {
                stripe_count: required_u64(&seg.params, "stripe_count", &context)
                    .map_err(SegmentParseError::Fatal)?,
            },
            &seg.params,
            &context,
            start_extent,
            extent_count,
            pv_names,
            lv_names,
        )?,
        "thin" => {
            parse_thin_segment(&seg.params, &context, lv_names).map_err(SegmentParseError::Fatal)?
        }
        "thin-pool" => parse_thin_pool_segment(&seg.params, &context, lv_names)
            .map_err(SegmentParseError::Fatal)?,
        "snapshot" => parse_snapshot_segment(&seg.params, &context, lv_names)
            .map_err(SegmentParseError::Fatal)?,
        "cache" => parse_cache_segment(&seg.params, &context, lv_names)
            .map_err(SegmentParseError::Fatal)?,
        "cache-pool" => parse_cache_pool_segment(&seg.params, &context, lv_names)
            .map_err(SegmentParseError::Fatal)?,
        other => ParsedSegmentParts {
            seg_type: SegmentType::Unsupported {
                type_name: other.to_string(),
            },
            stripes: Vec::new(),
            areas: parse_optional_areas(&seg.params, pv_names, lv_names).unwrap_or_default(),
            dependencies: SegmentDependencies::default(),
        },
    };

    let segment = SegmentMeta {
        start_extent,
        extent_count,
        seg_type,
        stripes,
        areas,
        dependencies,
    };
    if let Some(type_name) = unsupported_segment_type_name(&segment) {
        let reason = type_name.to_string();
        return Err(SegmentParseError::unsupported(segment, reason));
    }
    Ok(segment)
}

fn parse_raid_segment(
    seg_type: SegmentType,
    params: &[(String, String)],
    _context: &str,
    start_extent: u64,
    extent_count: u64,
    _pv_names: &HashSet<&str>,
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
        Err(err) => lvm_error_message(&err),
        Ok(raw) => lvm_error_message(
            &parse_raid_component_list(&raw, component_source, lv_names).unwrap_err(),
        ),
    };
    Err(SegmentParseError::unsupported(
        unsupported_segment(start_extent, extent_count, reason.clone()),
        reason,
    ))
}

fn raid_component_source(
    seg_type: &SegmentType,
) -> std::result::Result<RaidComponentSource, SegmentParseError> {
    match seg_type {
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

fn unsupported_raid_or_mirror_segment(
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

fn raid_segment_label(seg_type: &SegmentType) -> &'static str {
    match seg_type {
        SegmentType::Raid0 { .. } => "raid0",
        SegmentType::Raid1 { .. } => "raid1",
        SegmentType::Raid5 { .. } => "raid5",
        SegmentType::Raid6 { .. } => "raid6",
        SegmentType::Raid10 { .. } => "raid10",
        _ => "raid",
    }
}

fn parse_thin_segment(
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

fn parse_thin_pool_segment(
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

fn parse_cache_segment(
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

fn parse_cache_pool_segment(
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

fn parse_snapshot_segment(
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

fn unsupported_segment(start_extent: u64, extent_count: u64, reason: String) -> SegmentMeta {
    unsupported_segment_with_areas(start_extent, extent_count, reason, Vec::new())
}

fn unsupported_segment_with_areas(
    start_extent: u64,
    extent_count: u64,
    reason: String,
    areas: Vec<SegmentArea>,
) -> SegmentMeta {
    unsupported_segment_with_areas_and_dependencies(
        start_extent,
        extent_count,
        reason,
        areas,
        SegmentDependencies::default(),
    )
}

fn unsupported_segment_with_areas_and_dependencies(
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

fn lvm_error_message(err: &LvmError) -> String {
    match err {
        LvmError::MetadataParseError { message, .. } => message.clone(),
        other => other.to_string(),
    }
}

fn parse_required_stripes(
    params: &[(String, String)],
    context: &str,
    stripe_count: u64,
) -> Result<Vec<(String, u64)>> {
    let stripes_raw = required_string(params, "stripes", context)?;
    let stripes = parse_stripes_list(&stripes_raw, context)?;
    if stripes.len() != stripe_count as usize {
        return Err(LvmError::MetadataParseError {
            line: 0,
            message: format!(
                "{} expects {} stripe entries but found {}",
                context,
                stripe_count,
                stripes.len()
            ),
        });
    }
    Ok(stripes)
}

fn parse_linear_areas(
    params: &[(String, String)],
    context: &str,
    pv_names: &HashSet<&str>,
    lv_names: &HashSet<&str>,
) -> Result<StripeAreas> {
    let stripe_count = optional_u64(params, "stripe_count").unwrap_or(1);
    if stripe_count != 1 {
        return Err(LvmError::MetadataParseError {
            line: 0,
            message: format!("linear stripe_count must be 1 in {}", context),
        });
    }

    if params.iter().any(|(key, _)| key == "stripes") {
        let stripes = parse_required_stripes(params, context, stripe_count)?;
        let areas = resolve_stripe_areas(&stripes, pv_names, lv_names)?;
        return Ok((stripes_from_pv_areas(&areas), areas));
    }

    let areas = parse_optional_areas(params, pv_names, lv_names)?;
    if areas.len() != 1 {
        return Err(LvmError::MetadataParseError {
            line: 0,
            message: format!(
                "linear segment in {} expects exactly one stripes entry or one area, found {}",
                context,
                areas.len()
            ),
        });
    }

    Ok((stripes_from_pv_areas(&areas), areas))
}

fn required_stripe_size(params: &[(String, String)], context: &str) -> Result<u64> {
    let stripe_size = required_u64(params, "stripe_size", context)?;
    if stripe_size == 0 {
        return Err(LvmError::MetadataParseError {
            line: 0,
            message: format!("stripe_size must be greater than zero in {}", context),
        });
    }
    stripe_size
        .checked_mul(512)
        .ok_or_else(|| LvmError::MetadataParseError {
            line: 0,
            message: format!("stripe_size overflows bytes in {}", context),
        })?;
    Ok(stripe_size)
}

pub(super) fn unsupported_segment_type_name(segment: &SegmentMeta) -> Option<&str> {
    match &segment.seg_type {
        SegmentType::Unsupported { type_name } => Some(type_name.as_str()),
        SegmentType::ThinVolume => Some("thin"),
        SegmentType::ThinPool => Some("thin-pool"),
        SegmentType::Snapshot => Some("snapshot"),
        SegmentType::CacheVolume => Some("cache"),
        SegmentType::CachePool => Some("cache-pool"),
        SegmentType::Raid1 { .. } => Some("raid1"),
        SegmentType::Raid10 { .. } => Some("raid10"),
        SegmentType::Raid5 { .. } => Some("raid5"),
        SegmentType::Raid6 { .. } => Some("raid6"),
        SegmentType::Raid0 { .. } => Some("raid0"),
        SegmentType::Linear | SegmentType::Striped { .. } => None,
    }
}

pub(super) fn unsupported_lv_segment_with_areas_and_dependencies(
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

pub(super) fn merge_segment_dependencies(segs: &[SegmentMeta]) -> SegmentDependencies {
    let mut merged = SegmentDependencies::default();
    for segment in segs {
        merge_optional_string(&mut merged.thin_pool, &segment.dependencies.thin_pool);
        merge_optional_string(&mut merged.metadata, &segment.dependencies.metadata);
        merge_optional_string(&mut merged.pool, &segment.dependencies.pool);
        merge_optional_string(&mut merged.data, &segment.dependencies.data);
        merge_optional_string(&mut merged.origin, &segment.dependencies.origin);
        merge_optional_string(
            &mut merged.external_origin,
            &segment.dependencies.external_origin,
        );
        merge_optional_string(&mut merged.cow_store, &segment.dependencies.cow_store);
        merge_optional_string(
            &mut merged.merging_store,
            &segment.dependencies.merging_store,
        );
        merge_optional_string(&mut merged.cache_pool, &segment.dependencies.cache_pool);
        merge_optional_string(&mut merged.metadata_id, &segment.dependencies.metadata_id);
        merge_optional_string(&mut merged.data_id, &segment.dependencies.data_id);
        merged.transaction_id = merged
            .transaction_id
            .or(segment.dependencies.transaction_id);
        merged.device_id = merged.device_id.or(segment.dependencies.device_id);
        merged.chunk_size = merged.chunk_size.or(segment.dependencies.chunk_size);
        merged.metadata_format = merged
            .metadata_format
            .or(segment.dependencies.metadata_format);
        merged.metadata_start = merged
            .metadata_start
            .or(segment.dependencies.metadata_start);
        merged.metadata_len = merged.metadata_len.or(segment.dependencies.metadata_len);
        merged.data_start = merged.data_start.or(segment.dependencies.data_start);
        merged.data_len = merged.data_len.or(segment.dependencies.data_len);
        if merged.raid_component_source.is_none() {
            merged.raid_component_source = segment.dependencies.raid_component_source;
        }
        if merged.raid_components.is_empty() {
            merged.raid_components = segment.dependencies.raid_components.clone();
        }
    }
    merged
}

fn merge_optional_string(target: &mut Option<String>, source: &Option<String>) {
    if target.is_none() {
        *target = source.clone();
    }
}

fn resolve_stripe_areas(
    stripes: &[(String, u64)],
    pv_names: &HashSet<&str>,
    lv_names: &HashSet<&str>,
) -> Result<Vec<SegmentArea>> {
    let mut areas = Vec::with_capacity(stripes.len());
    for (name, start_extent) in stripes {
        if pv_names.contains(name.as_str()) {
            areas.push(SegmentArea::PhysicalVolume {
                name: name.clone(),
                start_extent: *start_extent,
            });
        } else if lv_names.contains(name.as_str()) {
            areas.push(SegmentArea::LogicalVolume {
                name: name.clone(),
                start_extent: *start_extent,
            });
        } else {
            return Err(LvmError::MetadataParseError {
                line: 0,
                message: format!("unknown LVM segment area '{}'", name),
            });
        }
    }
    Ok(areas)
}

fn stripes_from_pv_areas(areas: &[SegmentArea]) -> Vec<(String, u64)> {
    areas
        .iter()
        .filter_map(|area| match area {
            SegmentArea::PhysicalVolume { name, start_extent } => {
                Some((name.clone(), *start_extent))
            }
            SegmentArea::LogicalVolume { .. } | SegmentArea::Unassigned { .. } => None,
        })
        .collect()
}

fn parse_optional_areas(
    params: &[(String, String)],
    pv_names: &HashSet<&str>,
    lv_names: &HashSet<&str>,
) -> Result<Vec<SegmentArea>> {
    let Some((_, raw)) = params.iter().find(|(key, _)| key == "areas") else {
        return Ok(Vec::new());
    };
    parse_areas_list(raw, pv_names, lv_names)
}

fn parse_raid_component_areas(
    params: &[(String, String)],
    key: &str,
    source: RaidComponentSource,
    lv_names: &HashSet<&str>,
) -> Result<(Vec<SegmentArea>, Vec<RaidComponent>)> {
    let raw = required_string(params, key, "raid component list")?;
    parse_raid_component_list(&raw, source, lv_names)
}

fn parse_raid_component_list(
    raw: &str,
    source: RaidComponentSource,
    lv_names: &HashSet<&str>,
) -> Result<(Vec<SegmentArea>, Vec<RaidComponent>)> {
    let names = parse_component_names(raw, lv_names)?;
    let components = match source {
        RaidComponentSource::Raid0Lvs | RaidComponentSource::Stripes => names
            .iter()
            .map(|name| RaidComponent {
                data_lv: name.clone(),
                metadata_lv: None,
            })
            .collect(),
        RaidComponentSource::Raids => parse_raid_data_meta_pairs(&names),
    };
    let areas = names
        .into_iter()
        .map(|name| SegmentArea::LogicalVolume {
            name,
            start_extent: 0,
        })
        .collect();
    Ok((areas, components))
}

pub(super) fn max_segment_end(segs: &[SegmentMeta]) -> Result<u64> {
    let mut max_end = 0u64;
    for segment in segs {
        let end = segment
            .start_extent
            .checked_add(segment.extent_count)
            .ok_or_else(|| LvmError::MetadataParseError {
                line: 0,
                message: "logical volume extent range overflows u64".to_string(),
            })?;
        max_end = max_end.max(end);
    }
    Ok(max_end)
}

pub(super) fn validate_segment_layout(
    segs: &[SegmentMeta],
    context: &str,
) -> std::result::Result<(), String> {
    if segs.is_empty() {
        return Err(format!("{} contains no segment blocks", context));
    }

    let mut ranges = Vec::with_capacity(segs.len());
    for segment in segs {
        let end = segment
            .start_extent
            .checked_add(segment.extent_count)
            .ok_or_else(|| format!("{} segment extent range overflows u64", context))?;
        ranges.push((segment.start_extent, end));
    }
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
                "{} has segment {}: expected start_extent {} but found {}",
                context, relation, expected_start, start
            ));
        }
        expected_start = end;
    }

    Ok(())
}

/// Parse "pv0, 0, pv1, 1024" into [(pv0, 0), (pv1, 1024)].
fn parse_stripes_list(raw: &str, context: &str) -> Result<Vec<(String, u64)>> {
    let clean = raw.trim_matches(|c| c == '[' || c == ']' || c == '"');
    if clean.is_empty() {
        return Err(LvmError::MetadataParseError {
            line: 0,
            message: format!("missing stripes entries in {}", context),
        });
    }
    let parts: Vec<&str> = clean
        .split(',')
        .map(|s| s.trim().trim_matches('"'))
        .collect();
    if !parts.len().is_multiple_of(2) {
        return Err(LvmError::MetadataParseError {
            line: 0,
            message: format!("stripes list has an odd number of entries in {}", context),
        });
    }
    let mut result = Vec::new();
    let mut i = 0;
    while i + 1 < parts.len() {
        let pv = parts[i].to_string();
        if pv.is_empty() {
            return Err(LvmError::MetadataParseError {
                line: 0,
                message: format!("empty PV name in stripes list in {}", context),
            });
        }
        let extent = parts[i + 1]
            .parse::<u64>()
            .map_err(|_| LvmError::MetadataParseError {
                line: 0,
                message: format!("invalid stripe extent in {}", context),
            })?;
        result.push((pv, extent));
        i += 2;
    }
    Ok(result)
}

fn parse_areas_list(
    raw: &str,
    pv_names: &HashSet<&str>,
    lv_names: &HashSet<&str>,
) -> Result<Vec<SegmentArea>> {
    let clean = raw.trim_matches(|c| c == '[' || c == ']' || c == '"');
    if clean.is_empty() {
        return Ok(Vec::new());
    }
    let parts: Vec<&str> = clean
        .split(',')
        .map(|s| s.trim().trim_matches('"'))
        .collect();

    if parts.len().is_multiple_of(3) && looks_like_typed_areas_list(&parts) {
        return parse_typed_areas_list(&parts);
    }
    if parts.len().is_multiple_of(2) {
        return parse_untyped_areas_list(&parts, pv_names, lv_names);
    }

    Err(LvmError::MetadataParseError {
        line: 0,
        message: "areas list must contain pairs of area name and extent or triples of type, name, and extent"
            .to_string(),
    })
}

fn looks_like_typed_areas_list(parts: &[&str]) -> bool {
    parts.chunks_exact(3).all(|chunk| {
        matches!(
            chunk[0],
            "pv" | "PV"
                | "area_pv"
                | "AREA_PV"
                | "lv"
                | "LV"
                | "area_lv"
                | "AREA_LV"
                | "unassigned"
                | "UNASSIGNED"
                | "area_unassigned"
                | "AREA_UNASSIGNED"
        )
    })
}

fn parse_typed_areas_list(parts: &[&str]) -> Result<Vec<SegmentArea>> {
    let mut result = Vec::new();
    let mut i = 0;
    while i + 2 < parts.len() {
        let area_type = parts[i];
        let name = parts[i + 1];
        let start_extent =
            parts[i + 2]
                .parse::<u64>()
                .map_err(|_| LvmError::MetadataParseError {
                    line: 0,
                    message: "invalid extent in areas list".to_string(),
                })?;
        let area = match area_type {
            "pv" | "PV" | "area_pv" | "AREA_PV" => SegmentArea::PhysicalVolume {
                name: name.to_string(),
                start_extent,
            },
            "lv" | "LV" | "area_lv" | "AREA_LV" => SegmentArea::LogicalVolume {
                name: name.to_string(),
                start_extent,
            },
            "unassigned" | "UNASSIGNED" | "area_unassigned" | "AREA_UNASSIGNED" => {
                SegmentArea::Unassigned { start_extent }
            }
            other => {
                return Err(LvmError::MetadataParseError {
                    line: 0,
                    message: format!("unsupported LVM area type '{other}'"),
                });
            }
        };
        result.push(area);
        i += 3;
    }
    Ok(result)
}

fn parse_untyped_areas_list(
    parts: &[&str],
    pv_names: &HashSet<&str>,
    lv_names: &HashSet<&str>,
) -> Result<Vec<SegmentArea>> {
    let mut result = Vec::new();
    let mut i = 0;
    while i + 1 < parts.len() {
        let name = parts[i];
        if name.is_empty() {
            return Err(LvmError::MetadataParseError {
                line: 0,
                message: "empty LV name in areas list".to_string(),
            });
        }
        let start_extent =
            parts[i + 1]
                .parse::<u64>()
                .map_err(|_| LvmError::MetadataParseError {
                    line: 0,
                    message: "invalid extent in areas list".to_string(),
                })?;
        if pv_names.contains(name) {
            result.push(SegmentArea::PhysicalVolume {
                name: name.to_string(),
                start_extent,
            });
        } else if lv_names.contains(name) {
            result.push(SegmentArea::LogicalVolume {
                name: name.to_string(),
                start_extent,
            });
        } else {
            return Err(LvmError::MetadataParseError {
                line: 0,
                message: format!("unknown LVM segment area '{}'", name),
            });
        }
        i += 2;
    }
    Ok(result)
}

fn parse_component_names(raw: &str, lv_names: &HashSet<&str>) -> Result<Vec<String>> {
    let clean = raw.trim_matches(|c| c == '[' || c == ']' || c == '"');
    if clean.is_empty() {
        return Ok(Vec::new());
    }
    clean
        .split(',')
        .map(|item| item.trim().trim_matches('"'))
        .map(|name| {
            if name.is_empty() {
                return Err(LvmError::MetadataParseError {
                    line: 0,
                    message: "empty component LV name in raid component list".to_string(),
                });
            }
            if !lv_names.contains(name) {
                return Err(LvmError::MetadataParseError {
                    line: 0,
                    message: format!("unknown raid component logical volume '{}'", name),
                });
            }
            Ok(name.to_string())
        })
        .collect()
}

fn parse_raid_data_meta_pairs(names: &[String]) -> Vec<RaidComponent> {
    let mut components = Vec::new();
    let mut index = 0;
    while index < names.len() {
        if names[index].contains("_rmeta_") && index + 1 < names.len() {
            components.push(RaidComponent {
                data_lv: names[index + 1].clone(),
                metadata_lv: Some(names[index].clone()),
            });
            index += 2;
        } else {
            components.push(RaidComponent {
                data_lv: names[index].clone(),
                metadata_lv: None,
            });
            index += 1;
        }
    }
    components
}
