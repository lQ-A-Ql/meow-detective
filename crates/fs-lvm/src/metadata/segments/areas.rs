use std::collections::HashSet;

use crate::error::{LvmError, Result};
use crate::metadata::text::{optional_u64, required_string, required_u64};
use crate::metadata::{RaidComponent, RaidComponentSource, SegmentArea, SegmentMeta, SegmentType};

type StripeAreas = (Vec<(String, u64)>, Vec<SegmentArea>);

pub(super) fn parse_required_stripes(
    params: &[(String, String)],
    context: &str,
    stripe_count: u64,
) -> Result<Vec<(String, u64)>> {
    let raw = required_string(params, "stripes", context)?;
    let stripes = parse_stripes_list(&raw, context)?;
    if stripes.len() != stripe_count as usize {
        return Err(LvmError::MetadataParseError {
            line: 0,
            message: format!(
                "{context} expects {stripe_count} stripe entries but found {}",
                stripes.len()
            ),
        });
    }
    Ok(stripes)
}

pub(super) fn parse_linear_areas(
    params: &[(String, String)],
    context: &str,
    pv_names: &HashSet<&str>,
    lv_names: &HashSet<&str>,
) -> Result<StripeAreas> {
    let stripe_count = optional_u64(params, "stripe_count").unwrap_or(1);
    if stripe_count != 1 {
        return Err(LvmError::MetadataParseError {
            line: 0,
            message: format!("linear stripe_count must be 1 in {context}"),
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
                "linear segment in {context} expects exactly one stripes entry or one area, found {}",
                areas.len()
            ),
        });
    }
    Ok((stripes_from_pv_areas(&areas), areas))
}

pub(super) fn required_stripe_size(params: &[(String, String)], context: &str) -> Result<u64> {
    let stripe_size = required_u64(params, "stripe_size", context)?;
    if stripe_size == 0 {
        return Err(LvmError::MetadataParseError {
            line: 0,
            message: format!("stripe_size must be greater than zero in {context}"),
        });
    }
    stripe_size
        .checked_mul(512)
        .ok_or_else(|| LvmError::MetadataParseError {
            line: 0,
            message: format!("stripe_size overflows bytes in {context}"),
        })?;
    Ok(stripe_size)
}

pub(super) fn resolve_stripe_areas(
    stripes: &[(String, u64)],
    pv_names: &HashSet<&str>,
    lv_names: &HashSet<&str>,
) -> Result<Vec<SegmentArea>> {
    stripes
        .iter()
        .map(|(name, start_extent)| {
            if pv_names.contains(name.as_str()) {
                Ok(SegmentArea::PhysicalVolume {
                    name: name.clone(),
                    start_extent: *start_extent,
                })
            } else if lv_names.contains(name.as_str()) {
                Ok(SegmentArea::LogicalVolume {
                    name: name.clone(),
                    start_extent: *start_extent,
                })
            } else {
                Err(LvmError::MetadataParseError {
                    line: 0,
                    message: format!("unknown LVM segment area '{name}'"),
                })
            }
        })
        .collect()
}

pub(super) fn stripes_from_pv_areas(areas: &[SegmentArea]) -> Vec<(String, u64)> {
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

pub(super) fn parse_optional_areas(
    params: &[(String, String)],
    pv_names: &HashSet<&str>,
    lv_names: &HashSet<&str>,
) -> Result<Vec<SegmentArea>> {
    let Some((_, raw)) = params.iter().find(|(key, _)| key == "areas") else {
        return Ok(Vec::new());
    };
    parse_areas_list(raw, pv_names, lv_names)
}

pub(super) fn parse_raid_component_areas(
    params: &[(String, String)],
    key: &str,
    source: RaidComponentSource,
    lv_names: &HashSet<&str>,
) -> Result<(Vec<SegmentArea>, Vec<RaidComponent>)> {
    let raw = required_string(params, key, "raid component list")?;
    parse_raid_component_list(&raw, source, lv_names)
}

pub(super) fn parse_raid_component_list(
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

fn parse_stripes_list(raw: &str, context: &str) -> Result<Vec<(String, u64)>> {
    let clean = raw.trim_matches(|character| matches!(character, '[' | ']' | '"'));
    if clean.is_empty() {
        return Err(LvmError::MetadataParseError {
            line: 0,
            message: format!("missing stripes entries in {context}"),
        });
    }
    let parts = split_list(clean);
    if !parts.len().is_multiple_of(2) {
        return Err(LvmError::MetadataParseError {
            line: 0,
            message: format!("stripes list has an odd number of entries in {context}"),
        });
    }
    parts
        .chunks_exact(2)
        .map(|pair| {
            if pair[0].is_empty() {
                return Err(LvmError::MetadataParseError {
                    line: 0,
                    message: format!("empty PV name in stripes list in {context}"),
                });
            }
            let extent = pair[1]
                .parse::<u64>()
                .map_err(|_| LvmError::MetadataParseError {
                    line: 0,
                    message: format!("invalid stripe extent in {context}"),
                })?;
            Ok((pair[0].to_string(), extent))
        })
        .collect()
}

fn parse_areas_list(
    raw: &str,
    pv_names: &HashSet<&str>,
    lv_names: &HashSet<&str>,
) -> Result<Vec<SegmentArea>> {
    let clean = raw.trim_matches(|character| matches!(character, '[' | ']' | '"'));
    if clean.is_empty() {
        return Ok(Vec::new());
    }
    let parts = split_list(clean);
    if parts.len().is_multiple_of(3) && looks_like_typed_areas_list(&parts) {
        return parse_typed_areas_list(&parts);
    }
    if parts.len().is_multiple_of(2) {
        return parse_untyped_areas_list(&parts, pv_names, lv_names);
    }
    Err(LvmError::MetadataParseError {
        line: 0,
        message:
            "areas list must contain pairs of area name and extent or triples of type, name, and extent"
                .to_string(),
    })
}

fn split_list(raw: &str) -> Vec<&str> {
    raw.split(',')
        .map(|item| item.trim().trim_matches('"'))
        .collect()
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
    parts
        .chunks_exact(3)
        .map(|chunk| {
            let start_extent =
                chunk[2]
                    .parse::<u64>()
                    .map_err(|_| LvmError::MetadataParseError {
                        line: 0,
                        message: "invalid extent in areas list".to_string(),
                    })?;
            match chunk[0] {
                "pv" | "PV" | "area_pv" | "AREA_PV" => Ok(SegmentArea::PhysicalVolume {
                    name: chunk[1].to_string(),
                    start_extent,
                }),
                "lv" | "LV" | "area_lv" | "AREA_LV" => Ok(SegmentArea::LogicalVolume {
                    name: chunk[1].to_string(),
                    start_extent,
                }),
                "unassigned" | "UNASSIGNED" | "area_unassigned" | "AREA_UNASSIGNED" => {
                    Ok(SegmentArea::Unassigned { start_extent })
                }
                other => Err(LvmError::MetadataParseError {
                    line: 0,
                    message: format!("unsupported LVM area type '{other}'"),
                }),
            }
        })
        .collect()
}

fn parse_untyped_areas_list(
    parts: &[&str],
    pv_names: &HashSet<&str>,
    lv_names: &HashSet<&str>,
) -> Result<Vec<SegmentArea>> {
    parts
        .chunks_exact(2)
        .map(|pair| {
            let name = pair[0];
            if name.is_empty() {
                return Err(LvmError::MetadataParseError {
                    line: 0,
                    message: "empty LV name in areas list".to_string(),
                });
            }
            let start_extent =
                pair[1]
                    .parse::<u64>()
                    .map_err(|_| LvmError::MetadataParseError {
                        line: 0,
                        message: "invalid extent in areas list".to_string(),
                    })?;
            if pv_names.contains(name) {
                Ok(SegmentArea::PhysicalVolume {
                    name: name.to_string(),
                    start_extent,
                })
            } else if lv_names.contains(name) {
                Ok(SegmentArea::LogicalVolume {
                    name: name.to_string(),
                    start_extent,
                })
            } else {
                Err(LvmError::MetadataParseError {
                    line: 0,
                    message: format!("unknown LVM segment area '{name}'"),
                })
            }
        })
        .collect()
}

fn parse_component_names(raw: &str, lv_names: &HashSet<&str>) -> Result<Vec<String>> {
    let clean = raw.trim_matches(|character| matches!(character, '[' | ']' | '"'));
    if clean.is_empty() {
        return Ok(Vec::new());
    }
    split_list(clean)
        .into_iter()
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
                    message: format!("unknown raid component logical volume '{name}'"),
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

pub(crate) fn unsupported_segment_type_name(segment: &SegmentMeta) -> Option<&str> {
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
