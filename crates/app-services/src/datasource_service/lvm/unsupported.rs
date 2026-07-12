use super::super::{
    ImageFilesystemProbe, ImageFilesystemSource, UnsupportedImageKind, UnsupportedImageVolume,
};
use super::diagnostics::lvm_lv_diagnostic_context;
use super::model::ExpandedPoolSources;

pub(super) fn classify_unsupported_logical_volume(
    probe: &mut ImageFilesystemProbe,
    lv_reader: &mut dyn evidence_core::EvidenceReader,
    volume_group: &fs_lvm::VolumeGroup,
    logical_volume: &fs_lvm::LvInfo,
    expanded: &ExpandedPoolSources,
) {
    match super::super::has_bluestore_label(lv_reader) {
        Ok(true) => record_bluestore_volume(probe, volume_group, logical_volume, expanded),
        Ok(false) => record_unknown_volume(probe, volume_group, logical_volume, expanded),
        Err(error) => probe.warnings.push(format!(
            "LVM expand: unsupported-format detection failed for logical volume; {}: {}",
            lvm_lv_diagnostic_context(volume_group, logical_volume, &expanded.sources),
            error
        )),
    }
}

fn record_bluestore_volume(
    probe: &mut ImageFilesystemProbe,
    volume_group: &fs_lvm::VolumeGroup,
    logical_volume: &fs_lvm::LvInfo,
    expanded: &ExpandedPoolSources,
) {
    let volume_name = logical_volume_name(volume_group, logical_volume);
    probe.unsupported_volumes.push(UnsupportedImageVolume {
        kind: UnsupportedImageKind::CephBlueStore,
        source: ImageFilesystemSource::LvmLogicalVolume,
        name: Some(volume_name.clone()),
    });
    probe.warnings.push(format!(
        "LVM expand: Ceph BlueStore OSD logical volume detected and left unsupported; {}",
        lvm_lv_diagnostic_context(volume_group, logical_volume, &expanded.sources)
    ));
    tracing::info!("LVM LV '{volume_name}': Ceph BlueStore label detected");
}

fn record_unknown_volume(
    probe: &mut ImageFilesystemProbe,
    volume_group: &fs_lvm::VolumeGroup,
    logical_volume: &fs_lvm::LvInfo,
    expanded: &ExpandedPoolSources,
) {
    probe.warnings.push(format!(
        "LVM expand: no supported filesystem for logical volume; {}",
        lvm_lv_diagnostic_context(volume_group, logical_volume, &expanded.sources)
    ));
    tracing::debug!(
        "LVM LV '{}': no supported filesystem detected, skipping",
        logical_volume.name
    );
}

fn logical_volume_name(
    volume_group: &fs_lvm::VolumeGroup,
    logical_volume: &fs_lvm::LvInfo,
) -> String {
    if volume_group.name.is_empty() {
        logical_volume.name.clone()
    } else {
        format!("{}/{}", volume_group.name, logical_volume.name)
    }
}
