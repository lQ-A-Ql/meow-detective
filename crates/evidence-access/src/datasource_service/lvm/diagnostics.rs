use super::super::LvmPhysicalVolumeSource;
use super::model::LvmPvDiscoveryInfo;
use super::source_identity::{lvm_source_fingerprint, normalize_lvm_uuid_for_match};

pub(super) fn lvm_vg_diagnostic_context(
    volume_group: &fs_lvm::VolumeGroup,
    pv_sources: &[LvmPhysicalVolumeSource],
) -> String {
    format!(
        "VG name='{}' uuid='{}' PV source(s)={}",
        unknown_if_empty(&volume_group.name),
        unknown_if_empty(&volume_group.id),
        format_lvm_pv_sources(pv_sources)
    )
}

pub(super) fn lvm_lv_diagnostic_context(
    volume_group: &fs_lvm::VolumeGroup,
    lv_info: &fs_lvm::LvInfo,
    pv_sources: &[LvmPhysicalVolumeSource],
) -> String {
    format!(
        "VG name='{}' uuid='{}' LV name='{}' uuid='{}' role='{}' PV source(s)={}",
        unknown_if_empty(&volume_group.name),
        unknown_if_empty(&volume_group.id),
        unknown_if_empty(&lv_info.name),
        unknown_if_empty(&lv_info.uuid),
        unknown_if_empty(&lv_info.role),
        format_lvm_pv_sources(pv_sources)
    )
}

pub(super) fn format_lvm_pv_sources(sources: &[LvmPhysicalVolumeSource]) -> String {
    if sources.is_empty() {
        return "[]".to_string();
    }

    let rendered = sources
        .iter()
        .map(format_lvm_pv_source)
        .collect::<Vec<_>>()
        .join("; ");
    format!("[{rendered}]")
}

fn format_lvm_pv_source(source: &LvmPhysicalVolumeSource) -> String {
    format!(
        "PV name='{}' uuid='{}' source='{}' source_kind='{}' offset={}",
        source.pv_name.as_deref().unwrap_or("<unknown>"),
        unknown_if_empty(&source.pv_uuid),
        lvm_source_fingerprint(&source.source_path),
        source
            .source_kind
            .as_ref()
            .map(std::string::ToString::to_string)
            .unwrap_or_else(|| "<primary>".to_string()),
        source.offset
    )
}

pub(super) fn format_lvm_missing_pvs(missing_pvs: &[(String, String)]) -> String {
    if missing_pvs.is_empty() {
        return "[]".to_string();
    }

    let rendered = missing_pvs
        .iter()
        .map(|(pv_name, pv_uuid)| {
            format!(
                "PV name='{}' uuid='{}' source='<missing>' offset=<missing>",
                unknown_if_empty(pv_name),
                unknown_if_empty(pv_uuid)
            )
        })
        .collect::<Vec<_>>()
        .join("; ");
    format!("[{rendered}]")
}

pub(super) fn format_lvm_lv_summaries(volume_group: &fs_lvm::VolumeGroup) -> String {
    if volume_group.logical_volumes.is_empty() {
        return "[]".to_string();
    }

    let rendered = volume_group
        .logical_volumes
        .iter()
        .map(|lv| {
            format!(
                "LV name='{}' uuid='{}' role='{}'",
                unknown_if_empty(&lv.name),
                unknown_if_empty(&lv.uuid),
                lv.role.as_str()
            )
        })
        .collect::<Vec<_>>()
        .join("; ");
    format!("[{rendered}]")
}

pub(super) fn observed_lvm_sources_for_group(
    pv_infos: &[LvmPvDiscoveryInfo],
    volume_group: &fs_lvm::VolumeGroup,
) -> Vec<LvmPhysicalVolumeSource> {
    let group_key = lvm_volume_group_key(volume_group);
    pv_infos
        .iter()
        .filter(|info| {
            info.volume_group
                .as_ref()
                .is_some_and(|info_vg| lvm_volume_group_key(info_vg) == group_key)
        })
        .map(|info| lvm_source_with_vg_pv_name(&info.source, volume_group))
        .collect()
}

pub(super) fn lvm_volume_group_key(volume_group: &fs_lvm::VolumeGroup) -> String {
    let normalized_id = normalize_lvm_uuid_for_match(&volume_group.id);
    if normalized_id.is_empty() {
        format!("name:{}", volume_group.name)
    } else {
        format!("id:{normalized_id}")
    }
}

fn lvm_source_with_vg_pv_name(
    source: &LvmPhysicalVolumeSource,
    volume_group: &fs_lvm::VolumeGroup,
) -> LvmPhysicalVolumeSource {
    let mut source = source.clone();
    if source.pv_name.is_none() {
        let source_uuid = normalize_lvm_uuid_for_match(&source.pv_uuid);
        if let Some(pv_meta) = volume_group
            .physical_volumes
            .iter()
            .find(|pv_meta| normalize_lvm_uuid_for_match(&pv_meta.uuid) == source_uuid)
        {
            source.pv_name = Some(pv_meta.name.clone());
        }
    }
    source
}

fn unknown_if_empty(value: &str) -> &str {
    if value.is_empty() {
        "<unknown>"
    } else {
        value
    }
}
