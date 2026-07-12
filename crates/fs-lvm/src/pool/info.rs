use super::LvInfo;
use crate::metadata::{LvMeta, LvRole, SegmentArea, SegmentMeta, SegmentType};

pub(crate) fn lv_info_from_meta(logical_volume: &LvMeta) -> LvInfo {
    let visible = logical_volume.is_visible();
    let directly_mappable = logical_volume.is_directly_mappable();
    LvInfo {
        name: logical_volume.name.clone(),
        uuid: logical_volume.uuid.clone(),
        size_bytes: logical_volume.size_bytes,
        role: logical_volume.role.as_str().to_string(),
        status: logical_volume.status.clone(),
        visible,
        directly_mappable,
        unsupported_reason: unsupported_reason(logical_volume, visible, directly_mappable),
    }
}

fn unsupported_reason(
    logical_volume: &LvMeta,
    visible: bool,
    directly_mappable: bool,
) -> Option<String> {
    if directly_mappable {
        return None;
    }
    let unsupported_segments = logical_volume
        .segments
        .iter()
        .filter_map(unsupported_segment_label)
        .collect::<Vec<_>>();
    if !visible {
        Some("logical volume is hidden or internal".to_string())
    } else if !unsupported_segments.is_empty() {
        Some(format!(
            "logical volume uses unsupported segment(s): {}",
            unsupported_segments.join(", ")
        ))
    } else if matches!(logical_volume.role, LvRole::Snapshot) {
        Some("snapshot logical volume requires origin/COW mapping".to_string())
    } else {
        Some(format!(
            "logical volume role '{}' is not directly mappable",
            logical_volume.role.as_str()
        ))
    }
}

fn unsupported_segment_label(segment: &SegmentMeta) -> Option<String> {
    match &segment.seg_type {
        SegmentType::Unsupported { type_name } => {
            Some(unsupported_label_with_area_hint(type_name, segment))
        }
        SegmentType::ThinVolume => Some("thin".to_string()),
        SegmentType::ThinPool => Some("thin-pool".to_string()),
        SegmentType::Snapshot => Some("snapshot".to_string()),
        SegmentType::CacheVolume => Some("cache".to_string()),
        SegmentType::CachePool => Some("cache-pool".to_string()),
        SegmentType::Raid0 { .. } => Some("raid0".to_string()),
        SegmentType::Raid1 { .. } => Some("raid1".to_string()),
        SegmentType::Raid10 { .. } => Some("raid10".to_string()),
        SegmentType::Raid5 { .. } => Some("raid5".to_string()),
        SegmentType::Raid6 { .. } => Some("raid6".to_string()),
        SegmentType::Linear | SegmentType::Striped { .. } => None,
    }
}

fn unsupported_label_with_area_hint(type_name: &str, segment: &SegmentMeta) -> String {
    let component_lvs = segment
        .areas
        .iter()
        .filter_map(|area| match area {
            SegmentArea::LogicalVolume { name, .. } => Some(name.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();
    let mut dependency_lvs = segment.dependencies.referenced_lvs();
    dependency_lvs.sort_unstable();
    dependency_lvs.dedup();
    if component_lvs.is_empty() && dependency_lvs.is_empty() {
        return type_name.to_string();
    }
    let mut hints = Vec::new();
    if !component_lvs.is_empty() {
        hints.push(format!("areas={}", component_lvs.join(", ")));
    }
    if !dependency_lvs.is_empty() {
        hints.push(format!("dependencies={}", dependency_lvs.join(", ")));
    }
    format!("{type_name} (component LV graph: {})", hints.join("; "))
}
