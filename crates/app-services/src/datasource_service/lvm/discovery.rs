use super::super::probe::detect_image_filesystem;
use super::super::reader::open_evidence_reader;
use super::super::{
    ImageFilesystemCandidate, ImageFilesystemKind, LvmDiscoverySource, LvmPhysicalVolumeSource,
};
use super::diagnostics::{
    format_lvm_lv_summaries, format_lvm_missing_pvs, format_lvm_pv_sources,
    lvm_vg_diagnostic_context, lvm_volume_group_key, observed_lvm_sources_for_group,
};
use super::model::{LvmMetadataGroup, LvmPvDiscoveryInfo};
use super::source_identity::{
    lvm_pv_source_key, lvm_source_fingerprint, lvm_source_path_key, normalize_lvm_uuid_for_match,
};
use domain::DataSourceKind;
use std::collections::{BTreeMap, HashSet};
use std::io::{Read, Seek};
use std::path::Path;

type LvmPvSourceKey = (String, u64, String);

pub(super) fn lvm_discovery_pv_groups(
    lvm_indices: &[(usize, ImageFilesystemCandidate)],
    source_path: &Path,
    source_kind: &DataSourceKind,
    extra_sources: &[LvmDiscoverySource],
    warnings: &mut Vec<String>,
) -> Vec<Vec<LvmPhysicalVolumeSource>> {
    let (mut pv_infos, fallback_offsets) =
        inspect_primary_lvm_candidates(lvm_indices, source_path, source_kind, warnings);
    let primary_pv_keys = pv_infos
        .iter()
        .map(|info| lvm_pv_source_key(&info.source))
        .collect::<HashSet<_>>();
    inspect_extra_lvm_pv_sources(
        source_path,
        source_kind,
        extra_sources,
        &mut pv_infos,
        warnings,
    );
    append_metadata_warnings(&pv_infos, warnings);

    let metadata_groups = collect_metadata_groups(&pv_infos);
    let (mut groups, mut grouped_sources) =
        complete_metadata_groups(&pv_infos, &metadata_groups, &primary_pv_keys, warnings);
    append_metadata_free_groups(&mut groups, &mut grouped_sources, pv_infos);
    groups.extend(fallback_offsets);
    groups
}

fn inspect_primary_lvm_candidates(
    lvm_indices: &[(usize, ImageFilesystemCandidate)],
    source_path: &Path,
    source_kind: &DataSourceKind,
    warnings: &mut Vec<String>,
) -> (Vec<LvmPvDiscoveryInfo>, Vec<Vec<LvmPhysicalVolumeSource>>) {
    let mut pv_infos = Vec::new();
    let mut fallback_offsets = Vec::new();
    let default_source_path = source_path.to_string_lossy().into_owned();
    for (_, candidate) in lvm_indices {
        match inspect_lvm_pv_candidate(candidate, source_path, source_kind) {
            Ok(info) => pv_infos.push(info),
            Err(warning) => {
                warnings.push(warning);
                fallback_offsets.push(vec![LvmPhysicalVolumeSource {
                    source_path: default_source_path.clone(),
                    source_kind: Some(source_kind.clone()),
                    offset: candidate.offset,
                    pv_uuid: String::new(),
                    pv_name: None,
                }]);
            }
        }
    }
    (pv_infos, fallback_offsets)
}

fn append_metadata_warnings(pv_infos: &[LvmPvDiscoveryInfo], warnings: &mut Vec<String>) {
    for info in pv_infos {
        warnings.extend(info.metadata_warnings.iter().cloned());
    }
}

fn collect_metadata_groups(pv_infos: &[LvmPvDiscoveryInfo]) -> BTreeMap<String, LvmMetadataGroup> {
    let mut metadata_groups = BTreeMap::<String, LvmMetadataGroup>::new();
    for info in pv_infos {
        let Some(volume_group) = info.volume_group.as_ref() else {
            continue;
        };
        let key = lvm_volume_group_key(volume_group);
        metadata_groups
            .entry(key)
            .and_modify(|group| {
                if volume_group.seqno > group.volume_group.seqno {
                    group.volume_group = volume_group.clone();
                }
            })
            .or_insert_with(|| LvmMetadataGroup {
                volume_group: volume_group.clone(),
            });
    }
    metadata_groups
}

fn complete_metadata_groups(
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

fn inspect_extra_lvm_pv_sources(
    primary_source_path: &Path,
    primary_source_kind: &DataSourceKind,
    extra_sources: &[LvmDiscoverySource],
    pv_infos: &mut Vec<LvmPvDiscoveryInfo>,
    warnings: &mut Vec<String>,
) {
    if extra_sources.is_empty() {
        return;
    }

    let primary_key = lvm_source_path_key(primary_source_path);
    let mut seen = pv_infos
        .iter()
        .map(|info| lvm_pv_source_key(&info.source))
        .collect::<HashSet<_>>();
    let mut scanned_sources = HashSet::new();

    for source in extra_sources {
        if should_skip_extra_source(
            source,
            &primary_key,
            primary_source_kind,
            &mut scanned_sources,
        ) {
            continue;
        }
        inspect_extra_lvm_source(source, pv_infos, warnings, &mut seen);
    }
}

fn should_skip_extra_source(
    source: &LvmDiscoverySource,
    primary_key: &str,
    primary_source_kind: &DataSourceKind,
    scanned_sources: &mut HashSet<(String, String)>,
) -> bool {
    if matches!(source.source_kind, DataSourceKind::LogicalDirectory) {
        return true;
    }

    let source_path_key = lvm_source_path_key(&source.source_path);
    if source_path_key == primary_key && source.source_kind == *primary_source_kind {
        return true;
    }
    !scanned_sources.insert((source_path_key, source.source_kind.to_string()))
}

fn inspect_extra_lvm_source(
    source: &LvmDiscoverySource,
    pv_infos: &mut Vec<LvmPvDiscoveryInfo>,
    warnings: &mut Vec<String>,
    seen: &mut HashSet<LvmPvSourceKey>,
) {
    let mut reader = match open_evidence_reader(&source.source_path, &source.source_kind) {
        Ok(reader) => reader,
        Err(error) => {
            warnings.push(format!(
                "LVM expand: cannot open extra PV source='{}': {}",
                lvm_source_fingerprint(&source.source_path.to_string_lossy()),
                error
            ));
            return;
        }
    };
    let mut extra_probe = match detect_image_filesystem(&mut reader) {
        Ok(probe) => probe,
        Err(error) => {
            warnings.push(format!(
                "LVM expand: cannot inspect extra PV source='{}': {}",
                lvm_source_fingerprint(&source.source_path.to_string_lossy()),
                error
            ));
            return;
        }
    };

    for candidate in extra_probe
        .candidates
        .drain(..)
        .filter(|candidate| matches!(candidate.kind, ImageFilesystemKind::LvmPool))
    {
        match inspect_lvm_pv_candidate(&candidate, &source.source_path, &source.source_kind) {
            Ok(info) if seen.insert(lvm_pv_source_key(&info.source)) => pv_infos.push(info),
            Ok(_) => {}
            Err(warning) => warnings.push(warning),
        }
    }
}

fn inspect_lvm_pv_candidate(
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
    let (volume_group, metadata_warnings) =
        best_lvm_volume_group_from_label(&mut reader, candidate.offset, &label, source_path);
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
        metadata_warnings,
    })
}

fn best_lvm_volume_group_from_label<R>(
    reader: &mut R,
    pv_offset: u64,
    label: &fs_lvm::LvmLabel,
    source_path: &Path,
) -> (Option<fs_lvm::VolumeGroup>, Vec<String>)
where
    R: Read + Seek,
{
    let mut best = None;
    let mut warnings = Vec::new();
    for (index, metadata_area) in label.metadata_areas.iter().enumerate() {
        match fs_lvm::metadata::parse_metadata(reader, metadata_area, pv_offset) {
            Ok(volume_group) => {
                if best
                    .as_ref()
                    .is_none_or(|current: &fs_lvm::VolumeGroup| volume_group.seqno > current.seqno)
                {
                    best = Some(volume_group);
                }
            }
            Err(error) => warnings.push(format!(
                "LVM expand: metadata area {} for PV source='{}' offset={} pv_uuid='{}' (mda offset {}, size {}) did not produce a usable VG: {}",
                index,
                lvm_source_fingerprint(&source_path.to_string_lossy()),
                pv_offset,
                label.pv_uuid,
                metadata_area.offset,
                metadata_area.size,
                error
            )),
        }
    }
    (best, warnings)
}
