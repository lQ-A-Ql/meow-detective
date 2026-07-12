use std::collections::HashSet;

use crate::error::{LvmError, Result};

use super::syntax::Parser;
use super::types::LvSectionRaw;
use crate::metadata::segments::{
    max_segment_end, merge_segment_dependencies, parse_segment,
    unsupported_lv_segment_with_areas_and_dependencies, unsupported_segment_type_name,
    validate_segment_layout, SegmentParseError,
};
use crate::metadata::{LvMeta, LvRole, PvMeta, SegmentMeta, VolumeGroup};

pub(crate) fn parse_metadata_text(text: &str) -> Result<VolumeGroup> {
    let section = Parser::new(text).find_volume_group()?;
    let id = required_string(&section.params, "id", "volume group")?;
    let seqno = required_u64(&section.params, "seqno", "volume group")?;
    let extent_size = required_u64(&section.params, "extent_size", "volume group")?;
    let physical_volumes = parse_physical_volumes(&section.pv_sections)?;
    let pv_names = physical_volumes
        .iter()
        .map(|pv| pv.name.as_str())
        .collect::<HashSet<_>>();
    let lv_names = section
        .lv_sections
        .iter()
        .map(|lv| lv.name.as_str())
        .collect::<HashSet<_>>();
    let extent_size_bytes =
        extent_size
            .checked_mul(512)
            .ok_or_else(|| LvmError::MetadataParseError {
                line: 0,
                message: "volume group extent size overflows bytes".to_string(),
            })?;
    let logical_volumes = section
        .lv_sections
        .iter()
        .map(|lv| parse_logical_volume(lv, extent_size_bytes, &pv_names, &lv_names))
        .collect::<Result<Vec<_>>>()?;

    Ok(VolumeGroup {
        name: section.name,
        id,
        extent_size,
        seqno,
        physical_volumes,
        logical_volumes,
    })
}

fn parse_physical_volumes(sections: &[(String, Vec<(String, String)>)]) -> Result<Vec<PvMeta>> {
    sections
        .iter()
        .map(|(name, params)| {
            Ok(PvMeta {
                uuid: required_string(params, "id", &format!("physical volume '{name}'"))?,
                pe_start: required_u64(params, "pe_start", &format!("physical volume '{name}'"))?,
                pe_count: required_u64(params, "pe_count", &format!("physical volume '{name}'"))?,
                name: name.clone(),
            })
        })
        .collect()
}

fn parse_logical_volume(
    raw: &LvSectionRaw,
    extent_size_bytes: u64,
    pv_names: &HashSet<&str>,
    lv_names: &HashSet<&str>,
) -> Result<LvMeta> {
    let context = format!("logical volume '{}'", raw.name);
    let uuid = required_string(&raw.params, "id", &context)?;
    let status = optional_list(&raw.params, "status");
    let role = infer_lv_role(raw);
    let declared_segment_count = required_u64(&raw.params, "segment_count", &context)?;
    let (mut segments, mut unsupported_reason) = parse_segments(raw, &context, pv_names, lv_names)?;

    if declared_segment_count != raw.segments.len() as u64 {
        unsupported_reason = Some(format!(
            "{context} declares segment_count {declared_segment_count} but contains {} segment blocks",
            raw.segments.len()
        ));
    } else if let Some(type_name) = segments.iter().find_map(unsupported_segment_type_name) {
        unsupported_reason = Some(format!(
            "{context} uses unsupported segment type '{type_name}'"
        ));
    } else if segments
        .iter()
        .any(|segment| !segment.has_only_data_areas())
    {
        unsupported_reason = Some(format!(
            "{context} contains segment area(s) that are neither physical volumes nor logical-volume data areas"
        ));
    } else if let Err(error) = validate_segment_layout(&segments, &context) {
        unsupported_reason = Some(error);
    }

    if let Some(reason) = unsupported_reason {
        segments = collapse_unsupported_segments(&segments, reason)?;
    }
    let size_bytes = logical_volume_size_bytes(&segments, extent_size_bytes)?;
    Ok(LvMeta {
        name: raw.name.clone(),
        uuid,
        status,
        role,
        segments,
        size_bytes,
    })
}

fn parse_segments(
    raw: &LvSectionRaw,
    context: &str,
    pv_names: &HashSet<&str>,
    lv_names: &HashSet<&str>,
) -> Result<(Vec<SegmentMeta>, Option<String>)> {
    let mut segments = Vec::with_capacity(raw.segments.len());
    let mut unsupported_reason = None;
    for segment in &raw.segments {
        match parse_segment(segment, context, pv_names, lv_names) {
            Ok(metadata) => segments.push(metadata),
            Err(SegmentParseError::Unsupported { segment, reason }) => {
                segments.push(*segment);
                if unsupported_reason.is_none() {
                    unsupported_reason = Some(reason);
                }
            }
            Err(SegmentParseError::Fatal(error)) => return Err(error),
        }
    }
    Ok((segments, unsupported_reason))
}

fn collapse_unsupported_segments(
    segments: &[SegmentMeta],
    reason: String,
) -> Result<Vec<SegmentMeta>> {
    let size_extents = max_segment_end(segments)?;
    let areas = segments
        .iter()
        .flat_map(|segment| segment.areas.iter().cloned())
        .collect::<Vec<_>>();
    let dependencies = merge_segment_dependencies(segments);
    Ok(vec![unsupported_lv_segment_with_areas_and_dependencies(
        size_extents,
        reason,
        areas,
        dependencies,
    )])
}

fn optional_list(params: &[(String, String)], key: &str) -> Vec<String> {
    params
        .iter()
        .find(|(name, _)| name == key)
        .map(|(_, value)| parse_metadata_list_value(value))
        .unwrap_or_default()
}

fn parse_metadata_list_value(value: &str) -> Vec<String> {
    value
        .trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .split(',')
        .map(|item| item.trim().trim_matches('"').to_ascii_uppercase())
        .filter(|item| !item.is_empty())
        .collect()
}

fn infer_lv_role(raw: &LvSectionRaw) -> LvRole {
    let name = raw.name.as_str();
    if name.starts_with('[') && name.ends_with(']') {
        return LvRole::Internal;
    }
    if let Some(role) = infer_role_from_name(name) {
        return role;
    }
    if let Some(role) = infer_role_from_segments(raw) {
        return role;
    }
    let status = optional_list(&raw.params, "status");
    if !status.is_empty() && !status.iter().any(|item| item == "VISIBLE") {
        return LvRole::Internal;
    }
    LvRole::Public
}

fn infer_role_from_name(name: &str) -> Option<LvRole> {
    if name.ends_with("_tdata") {
        Some(LvRole::ThinData)
    } else if name.ends_with("_tmeta") {
        Some(LvRole::ThinMetadata)
    } else if name.ends_with("_cdata") {
        Some(LvRole::CacheData)
    } else if name.ends_with("_cmeta") {
        Some(LvRole::CacheMetadata)
    } else if name.contains("_rimage_") {
        Some(LvRole::RaidImage)
    } else if name.contains("_rmeta_") {
        Some(LvRole::RaidMetadata)
    } else if name.contains("_mimage_") {
        Some(LvRole::MirrorImage)
    } else if name.ends_with("_mlog") {
        Some(LvRole::MirrorLog)
    } else {
        None
    }
}

fn infer_role_from_segments(raw: &LvSectionRaw) -> Option<LvRole> {
    raw.segments.iter().find_map(|segment| {
        let segment_type = segment
            .params
            .iter()
            .find(|(key, _)| key == "type")
            .map(|(_, value)| value.as_str())?;
        match segment_type {
            "thin" => Some(LvRole::ThinVolume),
            "thin-pool" => Some(LvRole::ThinPool),
            "cache" => Some(LvRole::CacheVolume),
            "cache-pool" => Some(LvRole::CachePool),
            "snapshot" => Some(LvRole::Snapshot),
            _ => None,
        }
    })
}

pub(crate) fn required_string(
    params: &[(String, String)],
    key: &str,
    context: &str,
) -> Result<String> {
    params
        .iter()
        .find(|(name, _)| name == key)
        .map(|(_, value)| value.clone())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| LvmError::MetadataParseError {
            line: 0,
            message: format!("missing required field '{key}' in {context}"),
        })
}

pub(crate) fn required_u64(params: &[(String, String)], key: &str, context: &str) -> Result<u64> {
    required_string(params, key, context)?
        .parse::<u64>()
        .map_err(|_| LvmError::MetadataParseError {
            line: 0,
            message: format!("invalid integer field '{key}' in {context}"),
        })
}

pub(crate) fn optional_u64(params: &[(String, String)], key: &str) -> Option<u64> {
    params
        .iter()
        .find(|(name, _)| name == key)
        .and_then(|(_, value)| value.parse::<u64>().ok())
}

pub(crate) fn optional_string(params: &[(String, String)], key: &str) -> Option<String> {
    params
        .iter()
        .find(|(name, _)| name == key)
        .map(|(_, value)| value.clone())
        .filter(|value| !value.is_empty())
}

fn logical_volume_size_bytes(segments: &[SegmentMeta], extent_size_bytes: u64) -> Result<u64> {
    let max_end = segments.iter().try_fold(0u64, |max_end, segment| {
        let end_extent = segment
            .start_extent
            .checked_add(segment.extent_count)
            .ok_or_else(|| LvmError::MetadataParseError {
                line: 0,
                message: "logical volume extent range overflows u64".to_string(),
            })?;
        Ok::<_, LvmError>(max_end.max(end_extent))
    })?;
    max_end
        .checked_mul(extent_size_bytes)
        .ok_or_else(|| LvmError::MetadataParseError {
            line: 0,
            message: "logical volume byte size overflows u64".to_string(),
        })
}
