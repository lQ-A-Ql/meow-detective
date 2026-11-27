use super::super::reader::open_evidence_reader;
use super::super::{ImageFilesystemCandidate, LvmPhysicalVolumeSource};
use super::diagnostics::{
    format_lvm_lv_summaries, format_lvm_missing_pvs, format_lvm_pv_sources,
    lvm_vg_diagnostic_context, lvm_volume_group_key, observed_lvm_sources_for_group,
};
use super::model::{LvmMetadataGroup, LvmPvDiscoveryInfo};
use super::source_identity::{
    lvm_pv_source_key, lvm_source_fingerprint, normalize_lvm_uuid_for_match,
};
use domain::DataSourceKind;
use std::collections::{BTreeMap, HashSet};
use std::path::Path;

type LvmPvSourceKey = (String, u64, String);

pub(super) fn lvm_discovery_pv_groups(
    lvm_indices: &[(usize, ImageFilesystemCandidate)],
    source_path: &Path,
    source_kind: &DataSourceKind,
    warnings: &mut Vec<String>,
) -> Vec<Vec<LvmPhysicalVolumeSource>> {
    let pv_infos = inspect_primary_lvm_candidates(lvm_indices, source_path, source_kind, warnings);
    let primary_pv_keys = pv_infos
        .iter()
        .map(|info| lvm_pv_source_key(&info.source))
        .collect::<HashSet<_>>();
    let metadata_groups = match collect_metadata_groups(&pv_infos) {
        Ok(groups) => groups,
        Err(error) => {
            warnings.push(error.to_string());
            return Vec::new();
        }
    };
    let (mut groups, mut grouped_sources) =
        complete_metadata_groups(&pv_infos, &metadata_groups, &primary_pv_keys, warnings);
    append_metadata_free_groups(&mut groups, &mut grouped_sources, pv_infos);
    groups
}

fn inspect_primary_lvm_candidates(
    lvm_indices: &[(usize, ImageFilesystemCandidate)],
    source_path: &Path,
    source_kind: &DataSourceKind,
    warnings: &mut Vec<String>,
) -> Vec<LvmPvDiscoveryInfo> {
    let mut pv_infos = Vec::new();
    for (_, candidate) in lvm_indices {
        match inspect_lvm_pv_candidate(candidate, source_path, source_kind) {
            Ok(info) => pv_infos.push(info),
            Err(warning) => warnings.push(warning),
        }
    }
    pv_infos
}

pub(super) fn collect_metadata_groups(
    pv_infos: &[LvmPvDiscoveryInfo],
) -> Result<BTreeMap<String, LvmMetadataGroup>, fs_lvm::LvmError> {
    let mut metadata_groups = BTreeMap::<String, LvmMetadataGroup>::new();
    for info in pv_infos {
        let Some(volume_group) = info.volume_group.as_ref() else {
            continue;
        };
        let key = lvm_volume_group_key(volume_group);
        if let Some(group) = metadata_groups.get_mut(&key) {
            if let Some(selected) =
                fs_lvm::metadata::select_committed_metadata([&group.volume_group, volume_group])?
            {
                group.volume_group = selected.clone();
            }
        } else {
            metadata_groups.insert(
                key,
                LvmMetadataGroup {
                    volume_group: volume_group.clone(),
                },
            );
        }
    }
    Ok(metadata_groups)
}

pub(super) fn complete_metadata_groups(
    pv_infos: &[LvmPvDiscoveryInfo],
    metadata_groups: &BTreeMap<String, LvmMetadataGroup>,
    primary_pv_keys: &HashSet<LvmPvSourceKey>,
    warnings: &mut Vec<String>,
) -> (Vec<Vec<LvmPhysicalVolumeSource>>, HashSet<LvmPvSourceKey>) {
    let mut grouped_sources = HashSet::new();
    let mut groups = Vec::new();
    for group in metadata_groups.values() {
        let (sources, missing_pv_uuids) =
            sources_required_by_volume_group(pv_infos, &group.volume_group);
        if missing_pv_uuids.is_empty() && !sources.is_empty() {
            append_complete_group(&mut groups, &mut grouped_sources, primary_pv_keys, sources);
        } else if !missing_pv_uuids.is_empty() {
            warn_incomplete_group(pv_infos, &group.volume_group, &missing_pv_uuids, warnings);
        }
    }
    (groups, grouped_sources)
}

fn sources_required_by_volume_group(
    pv_infos: &[LvmPvDiscoveryInfo],
    volume_group: &fs_lvm::VolumeGroup,
) -> (Vec<LvmPhysicalVolumeSource>, Vec<(String, String)>) {
    let mut sources = Vec::new();
    let mut missing_pv_uuids = Vec::new();
    for pv_meta in &volume_group.physical_volumes {
        let required_uuid = normalize_lvm_uuid_for_match(&pv_meta.uuid);
        let matched = pv_infos
            .iter()
            .find(|info| normalize_lvm_uuid_for_match(&info.label.pv_uuid) == required_uuid);
        match matched {
            Some(info) => append_unique_vg_source(&mut sources, info, &pv_meta.name),
            None => missing_pv_uuids.push((pv_meta.name.clone(), pv_meta.uuid.clone())),
        }
    }
    (sources, missing_pv_uuids)
}

fn append_unique_vg_source(
    sources: &mut Vec<LvmPhysicalVolumeSource>,
    info: &LvmPvDiscoveryInfo,
    pv_name: &str,
) {
    if sources
        .iter()
        .any(|source| lvm_pv_source_key(source) == lvm_pv_source_key(&info.source))
    {
        return;
    }

    let mut source = info.source.clone();
    source.pv_name = Some(pv_name.to_string());
    sources.push(source);
}

fn append_complete_group(
    groups: &mut Vec<Vec<LvmPhysicalVolumeSource>>,
    grouped_sources: &mut HashSet<LvmPvSourceKey>,
    primary_pv_keys: &HashSet<LvmPvSourceKey>,
    sources: Vec<LvmPhysicalVolumeSource>,
) {
    if !sources
        .iter()
        .any(|source| primary_pv_keys.contains(&lvm_pv_source_key(source)))
    {
        return;
    }
    for source in &sources {
        grouped_sources.insert(lvm_pv_source_key(source));
    }
    groups.push(sources);
}

fn warn_incomplete_group(
    pv_infos: &[LvmPvDiscoveryInfo],
    volume_group: &fs_lvm::VolumeGroup,
    missing_pv_uuids: &[(String, String)],
    warnings: &mut Vec<String>,
) {
    let observed_sources = observed_lvm_sources_for_group(pv_infos, volume_group);
    warnings.push(format!(
        "LVM expand: skipping incomplete {}; missing PV source(s)={}; observed PV source(s)={}; LV candidate(s)={}",
        lvm_vg_diagnostic_context(volume_group, &observed_sources),
        format_lvm_missing_pvs(missing_pv_uuids),
        format_lvm_pv_sources(&observed_sources),
        format_lvm_lv_summaries(volume_group)
    ));
}

fn append_metadata_free_groups(
    groups: &mut Vec<Vec<LvmPhysicalVolumeSource>>,
    grouped_sources: &mut HashSet<LvmPvSourceKey>,
    pv_infos: Vec<LvmPvDiscoveryInfo>,
) {
    for info in pv_infos {
        if info.volume_group.is_none() && grouped_sources.insert(lvm_pv_source_key(&info.source)) {
            groups.push(vec![info.source]);
        }
    }
}

pub(super) fn inspect_lvm_pv_candidate(
    candidate: &ImageFilesystemCandidate,
    source_path: &Path,
    source_kind: &DataSourceKind,
) -> std::result::Result<LvmPvDiscoveryInfo, String> {
    let mut reader = open_evidence_reader(source_path, source_kind).map_err(|e| {
        format!(
            "LVM expand: cannot open reader for PV source='{}' offset={}: {}",
            lvm_source_fingerprint(&source_path.to_string_lossy()),
            candidate.offset,
            e
        )
    })?;
    let label = fs_lvm::label::parse_pv_label(&mut reader, candidate.offset).map_err(|e| {
        format!(
            "LVM expand: cannot parse PV label for source='{}' offset={}: {}",
            lvm_source_fingerprint(&source_path.to_string_lossy()),
            candidate.offset,
            e
        )
    })?;
    let volume_group = if label.metadata_areas.is_empty() {
        None
    } else {
        Some(
            fs_lvm::metadata::parse_metadata_from_regions(
                &mut reader,
                &label.metadata_areas,
                candidate.offset,
            )
            .map_err(|error| {
                format!(
                    "LVM committed metadata invalid for PV '{}': {error}",
                    label.pv_uuid
                )
            })?,
        )
    };
    let source = LvmPhysicalVolumeSource {
        source_path: source_path.to_string_lossy().into_owned(),
        source_kind: Some(source_kind.clone()),
        offset: candidate.offset,
        pv_uuid: label.pv_uuid.clone(),
        pv_name: None,
    };
    Ok(LvmPvDiscoveryInfo {
        source,
        label,
        volume_group,
    })
}
