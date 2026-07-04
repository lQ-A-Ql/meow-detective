use super::fs_magic::{kind_label, read_boot_filesystem};
use super::probe::detect_image_filesystem;
use super::reader::open_evidence_reader;
use super::{
    ImageFilesystemCandidate, ImageFilesystemKind, ImageFilesystemProbe, ImageFilesystemSource,
    LvmDiscoverySource, LvmLogicalVolumeIdentity, LvmPhysicalVolumeSource, PartitionRecord,
    PartitionStatus,
};
use domain::DataSourceKind;
use std::io::{Read, Seek};
use std::path::Path;

/// Expand LVM pool candidates into individual logical volume candidates.
///
/// Groups `LvmPool` candidates by VG metadata before discovery so an
/// incomplete/high-seqno VG cannot prevent a separate complete VG from
/// expanding.
///
/// Call after `detect_image_filesystem` and before storing partition records.
pub fn expand_lvm_pool_candidates(
    probe: &mut ImageFilesystemProbe,
    source_path: &Path,
    source_kind: &DataSourceKind,
) {
    expand_lvm_pool_candidates_with_sources(probe, source_path, source_kind, &[]);
}

pub fn expand_lvm_pool_candidates_with_sources(
    probe: &mut ImageFilesystemProbe,
    source_path: &Path,
    source_kind: &DataSourceKind,
    extra_sources: &[LvmDiscoverySource],
) {
    let lvm_indices: Vec<(usize, ImageFilesystemCandidate)> = probe
        .candidates
        .iter()
        .enumerate()
        .filter(|(_, c)| matches!(c.kind, ImageFilesystemKind::LvmPool))
        .map(|(i, c)| (i, c.clone()))
        .collect();

    if lvm_indices.is_empty() {
        return;
    }

    let mut new_candidates: Vec<(ImageFilesystemCandidate, u64)> = Vec::new();
    let mut remove_indices: Vec<usize> = Vec::new();
    let mut expanded_vgs = std::collections::HashSet::new();
    let discovery_groups = lvm_discovery_pv_groups(
        &lvm_indices,
        source_path,
        source_kind,
        extra_sources,
        &mut probe.warnings,
    );

    for pv_sources in discovery_groups {
        let pv_offsets = pv_sources
            .iter()
            .map(|source| source.offset)
            .collect::<Vec<_>>();
        let seed_offset = pv_offsets.first().copied().unwrap_or_default();
        let mut readers = Vec::with_capacity(pv_sources.len());
        for pv_source in &pv_sources {
            let reader_path = Path::new(&pv_source.source_path);
            let reader_kind = pv_source.source_kind.as_ref().unwrap_or(source_kind);
            match open_evidence_reader(reader_path, reader_kind) {
                Ok(reader) => readers.push(reader),
                Err(e) => {
                    probe.warnings.push(format!(
                        "LVM expand: cannot open reader for PV source='{}' offset {}: {}",
                        lvm_source_fingerprint(&pv_source.source_path),
                        pv_source.offset,
                        e
                    ));
                    tracing::warn!(
                        "LVM expand: cannot open reader for PV source='{}' at offset {}: {}",
                        lvm_source_fingerprint(&pv_source.source_path),
                        pv_source.offset,
                        e
                    );
                    readers.clear();
                    break;
                }
            }
        }
        if readers.is_empty() {
            continue;
        }

        let pool = match fs_lvm::LvmPool::discover(readers, pv_offsets.clone()) {
            Ok(p) => p,
            Err(e) => {
                probe.warnings.push(format!(
                    "LVM expand: discovery failed for PV source(s) {}: {}",
                    format_lvm_pv_sources(&pv_sources),
                    e
                ));
                tracing::warn!(
                    "LVM expand: discovery failed for PV source(s) {}: {}",
                    format_lvm_pv_sources(&pv_sources),
                    e
                );
                continue;
            }
        };

        let vg_pv_mappings = pool
            .physical_volume_offsets()
            .iter()
            .map(|(pv_name, offset)| (pv_name.clone(), *offset))
            .collect::<Vec<_>>();
        let expanded_offsets = if vg_pv_mappings.is_empty() {
            vec![seed_offset]
        } else {
            vg_pv_mappings
                .iter()
                .map(|(_, offset)| *offset)
                .collect::<Vec<_>>()
        };
        let mut expanded_sources =
            lvm_sources_for_pv_mappings(&pv_sources, &vg_pv_mappings, &expanded_offsets);
        if expanded_sources.len() != pv_offsets.len() {
            expanded_sources = pv_sources.clone();
        }
        let expanded_offsets = expanded_sources
            .iter()
            .map(|source| source.offset)
            .collect::<Vec<_>>();
        let primary_expanded_offsets = expanded_sources
            .iter()
            .filter(|source| lvm_source_matches(source, source_path, source_kind))
            .map(|source| source.offset)
            .collect::<Vec<_>>();
        let representative_offsets = if primary_expanded_offsets.is_empty() {
            &expanded_offsets
        } else {
            &primary_expanded_offsets
        };
        let representative = representative_lvm_candidate(&lvm_indices, representative_offsets)
            .or_else(|| {
                lvm_indices
                    .iter()
                    .find(|(_, candidate)| candidate.offset == seed_offset)
                    .map(|(_, candidate)| candidate)
            });
        let candidate_offset = primary_expanded_offsets
            .first()
            .copied()
            .or_else(|| expanded_offsets.first().copied())
            .unwrap_or(seed_offset);

        let vg = pool.volume_group();
        let vg_key = if vg.id.is_empty() {
            vg.name.clone()
        } else {
            vg.id.clone()
        };
        if !expanded_vgs.insert(vg_key) {
            mark_lvm_partitions_expanded(probe, &primary_expanded_offsets);
            remove_lvm_candidates_for_offsets(
                &mut remove_indices,
                &lvm_indices,
                &primary_expanded_offsets,
            );
            continue;
        }

        let lv_list = pool.list_volumes();
        tracing::info!(
            "LVM: {} logical volume(s) discovered at offset {}",
            lv_list.len(),
            expanded_offsets.first().copied().unwrap_or(seed_offset),
        );
        for lv_info in &lv_list {
            if !lv_info.directly_mappable {
                let reason = lv_info
                    .unsupported_reason
                    .as_deref()
                    .unwrap_or("unsupported logical volume mapping");
                probe.warnings.push(format!(
                    "LVM expand: skipping unsupported logical volume; {}: {}",
                    lvm_lv_diagnostic_context(vg, lv_info, &expanded_sources),
                    reason
                ));
                tracing::debug!(
                    "LVM: skipping logical volume '{}' role='{}': {}",
                    lv_info.name,
                    lv_info.role,
                    reason
                );
            }
        }

        let candidates_before = new_candidates.len();
        for (lv_idx, lv_info) in pool.list_direct_volumes() {
            let mut lv_reader = match pool.open_volume(lv_idx) {
                Ok(r) => r,
                Err(e) => {
                    probe.warnings.push(format!(
                        "LVM expand: open logical volume failed; {}: {}",
                        lvm_lv_diagnostic_context(vg, &lv_info, &expanded_sources),
                        e
                    ));
                    tracing::warn!("LVM: open_volume '{}' failed: {}", lv_info.name, e);
                    continue;
                }
            };

            match read_boot_filesystem(&mut lv_reader, 0) {
                Ok(Some(fs_kind)) if !matches!(fs_kind, ImageFilesystemKind::LvmPool) => {
                    let lv_name = format!(
                        "{}/{}",
                        if vg.name.is_empty() {
                            representative
                                .and_then(|candidate| candidate.partition_name.as_deref())
                                .unwrap_or("LVM")
                        } else {
                            vg.name.as_str()
                        },
                        lv_info.name
                    );
                    let lvm_identity = LvmLogicalVolumeIdentity {
                        vg_uuid: vg.id.clone(),
                        vg_name: vg.name.clone(),
                        lv_uuid: lv_info.uuid.clone(),
                        lv_name: lv_info.name.clone(),
                        pv_offsets: expanded_offsets.clone(),
                        pv_sources: expanded_sources.clone(),
                    };
                    new_candidates.push((
                        ImageFilesystemCandidate {
                            partition_index: representative
                                .and_then(|candidate| candidate.partition_index),
                            partition_name: Some(lv_name),
                            kind: fs_kind,
                            offset: candidate_offset,
                            source: ImageFilesystemSource::LvmLogicalVolume,
                            lvm_identity: Some(lvm_identity),
                        },
                        lv_info.size_bytes,
                    ));
                }
                Ok(_) => {
                    probe.warnings.push(format!(
                        "LVM expand: no supported filesystem for logical volume; {}",
                        lvm_lv_diagnostic_context(vg, &lv_info, &expanded_sources)
                    ));
                    tracing::debug!(
                        "LVM LV '{}': no supported filesystem detected, skipping",
                        lv_info.name
                    );
                }
                Err(e) => {
                    probe.warnings.push(format!(
                        "LVM expand: filesystem detection failed for logical volume; {}: {}",
                        lvm_lv_diagnostic_context(vg, &lv_info, &expanded_sources),
                        e
                    ));
                    tracing::debug!("LVM LV '{}': FS detection error: {}", lv_info.name, e);
                }
            }
        }

        mark_lvm_partitions_expanded(probe, &primary_expanded_offsets);
        remove_lvm_candidates_for_offsets(
            &mut remove_indices,
            &lvm_indices,
            &primary_expanded_offsets,
        );
        if new_candidates.len() == candidates_before {
            probe.warnings.push(format!(
                "LVM expand: volume group produced no supported logical volume candidates; {} LV candidate(s)={}",
                lvm_vg_diagnostic_context(vg, &expanded_sources),
                format_lvm_lv_summaries(vg)
            ));
        }
    }

    remove_indices.sort_unstable_by(|a, b| b.cmp(a));
    for idx in &remove_indices {
        probe.candidates.remove(*idx);
    }

    let next_index = probe.partitions.iter().map(|p| p.index).max().unwrap_or(0) + 1;
    for (i, (lv_candidate, lv_size_bytes)) in new_candidates.iter_mut().enumerate() {
        let lv_index = next_index + i;
        lv_candidate.partition_index = Some(lv_index);
        probe.partitions.push(PartitionRecord {
            index: lv_index,
            name: lv_candidate
                .partition_name
                .clone()
                .unwrap_or_else(|| format!("LV_{}", lv_index)),
            kind_label: kind_label(lv_candidate.kind),
            type_guid: None,
            offset: lv_candidate.offset,
            length: *lv_size_bytes,
            status: PartitionStatus::Supported,
            filesystem: Some(lv_candidate.kind),
            lvm_identity: lv_candidate.lvm_identity.clone(),
        });
    }

    probe
        .candidates
        .extend(new_candidates.into_iter().map(|(candidate, _)| candidate));
}

#[derive(Clone)]
struct LvmPvDiscoveryInfo {
    source: LvmPhysicalVolumeSource,
    label: fs_lvm::LvmLabel,
    volume_group: Option<fs_lvm::VolumeGroup>,
    metadata_warnings: Vec<String>,
}

struct LvmMetadataGroup {
    volume_group: fs_lvm::VolumeGroup,
}

fn lvm_discovery_pv_groups(
    lvm_indices: &[(usize, ImageFilesystemCandidate)],
    source_path: &Path,
    source_kind: &DataSourceKind,
    extra_sources: &[LvmDiscoverySource],
    warnings: &mut Vec<String>,
) -> Vec<Vec<LvmPhysicalVolumeSource>> {
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
    let primary_pv_keys = pv_infos
        .iter()
        .map(|info| lvm_pv_source_key(&info.source))
        .collect::<std::collections::HashSet<_>>();
    inspect_extra_lvm_pv_sources(
        source_path,
        source_kind,
        extra_sources,
        &mut pv_infos,
        warnings,
    );

    for info in &pv_infos {
        warnings.extend(info.metadata_warnings.iter().cloned());
    }

    let mut metadata_groups = std::collections::BTreeMap::<String, LvmMetadataGroup>::new();
    for info in &pv_infos {
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

    let mut grouped_offsets = std::collections::HashSet::new();
    let mut groups = Vec::new();
    for group in metadata_groups.values() {
        let mut sources = Vec::new();
        let mut missing_pv_uuids = Vec::new();
        for pv_meta in &group.volume_group.physical_volumes {
            let required_uuid = normalize_lvm_uuid_for_match(&pv_meta.uuid);
            let matched = pv_infos
                .iter()
                .find(|info| normalize_lvm_uuid_for_match(&info.label.pv_uuid) == required_uuid);
            match matched {
                Some(info) => {
                    if !sources.iter().any(|source: &LvmPhysicalVolumeSource| {
                        lvm_pv_source_key(source) == lvm_pv_source_key(&info.source)
                    }) {
                        let mut source = info.source.clone();
                        source.pv_name = Some(pv_meta.name.clone());
                        sources.push(source);
                    }
                }
                None => missing_pv_uuids.push((pv_meta.name.clone(), pv_meta.uuid.clone())),
            }
        }

        if missing_pv_uuids.is_empty() && !sources.is_empty() {
            if !sources
                .iter()
                .any(|source| primary_pv_keys.contains(&lvm_pv_source_key(source)))
            {
                continue;
            }
            for source in &sources {
                grouped_offsets.insert(lvm_pv_source_key(source));
            }
            groups.push(sources);
        } else if !missing_pv_uuids.is_empty() {
            let observed_sources = observed_lvm_sources_for_group(&pv_infos, &group.volume_group);
            warnings.push(format!(
                "LVM expand: skipping incomplete {}; missing PV source(s)={}; observed PV source(s)={}; LV candidate(s)={}",
                lvm_vg_diagnostic_context(&group.volume_group, &observed_sources),
                format_lvm_missing_pvs(&missing_pv_uuids),
                format_lvm_pv_sources(&observed_sources),
                format_lvm_lv_summaries(&group.volume_group)
            ));
        }
    }

    for info in pv_infos {
        if info.volume_group.is_none() && grouped_offsets.insert(lvm_pv_source_key(&info.source)) {
            groups.push(vec![info.source]);
        }
    }
    groups.extend(fallback_offsets);
    groups
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
        .collect::<std::collections::HashSet<_>>();
    let mut scanned_sources = std::collections::HashSet::new();

    for source in extra_sources {
        if matches!(source.source_kind, DataSourceKind::LogicalDirectory) {
            continue;
        }

        let source_path_key = lvm_source_path_key(&source.source_path);
        if source_path_key == primary_key && source.source_kind == *primary_source_kind {
            continue;
        }
        if !scanned_sources.insert((source_path_key, source.source_kind.to_string())) {
            continue;
        }

        let mut reader = match open_evidence_reader(&source.source_path, &source.source_kind) {
            Ok(reader) => reader,
            Err(error) => {
                warnings.push(format!(
                    "LVM expand: cannot open extra PV source='{}': {}",
                    lvm_source_fingerprint(&source.source_path.to_string_lossy()),
                    error
                ));
                continue;
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
                continue;
            }
        };

        let extra_lvm_candidates = extra_probe
            .candidates
            .drain(..)
            .filter(|candidate| matches!(candidate.kind, ImageFilesystemKind::LvmPool))
            .collect::<Vec<_>>();
        for candidate in extra_lvm_candidates {
            match inspect_lvm_pv_candidate(&candidate, &source.source_path, &source.source_kind) {
                Ok(info) => {
                    if seen.insert(lvm_pv_source_key(&info.source)) {
                        pv_infos.push(info);
                    }
                }
                Err(warning) => warnings.push(warning),
            }
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
            Err(error) => {
                warnings.push(format!(
                    "LVM expand: metadata area {} for PV source='{}' offset={} pv_uuid='{}' (mda offset {}, size {}) did not produce a usable VG: {}",
                    index,
                    lvm_source_fingerprint(&source_path.to_string_lossy()),
                    pv_offset,
                    label.pv_uuid,
                    metadata_area.offset,
                    metadata_area.size,
                    error
                ));
            }
        }
    }
    (best, warnings)
}

fn lvm_volume_group_key(volume_group: &fs_lvm::VolumeGroup) -> String {
    let normalized_id = normalize_lvm_uuid_for_match(&volume_group.id);
    if normalized_id.is_empty() {
        format!("name:{}", volume_group.name)
    } else {
        format!("id:{normalized_id}")
    }
}

fn lvm_vg_diagnostic_context(
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

fn lvm_lv_diagnostic_context(
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

fn format_lvm_pv_sources(sources: &[LvmPhysicalVolumeSource]) -> String {
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

fn format_lvm_missing_pvs(missing_pvs: &[(String, String)]) -> String {
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

fn format_lvm_lv_summaries(volume_group: &fs_lvm::VolumeGroup) -> String {
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

fn observed_lvm_sources_for_group(
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

pub(crate) fn lvm_source_fingerprint(source_path: &str) -> String {
    if source_path.is_empty() {
        return "<unknown>".to_string();
    }

    let digest = <sha2::Sha256 as sha2::Digest>::digest(source_path.as_bytes());
    let short_hash = digest
        .iter()
        .take(8)
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("path-sha256:{short_hash}")
}

pub(crate) fn normalize_lvm_uuid_for_match(uuid: &str) -> String {
    uuid.trim()
        .chars()
        .filter(|ch| *ch != '-')
        .collect::<String>()
        .to_ascii_lowercase()
}

pub(super) fn lvm_pv_source_key(source: &LvmPhysicalVolumeSource) -> (String, u64, String) {
    (
        format!(
            "{}|{}",
            source
                .source_kind
                .as_ref()
                .map(std::string::ToString::to_string)
                .unwrap_or_default(),
            source.source_path
        ),
        source.offset,
        normalize_lvm_uuid_for_match(&source.pv_uuid),
    )
}

fn lvm_source_path_key(source_path: &Path) -> String {
    std::fs::canonicalize(source_path)
        .unwrap_or_else(|_| source_path.to_path_buf())
        .to_string_lossy()
        .to_string()
        .to_ascii_lowercase()
}

fn lvm_source_matches(
    source: &LvmPhysicalVolumeSource,
    source_path: &Path,
    source_kind: &DataSourceKind,
) -> bool {
    source
        .source_kind
        .as_ref()
        .is_none_or(|kind| kind == source_kind)
        && lvm_source_path_key(Path::new(&source.source_path)) == lvm_source_path_key(source_path)
}

fn representative_lvm_candidate<'a>(
    lvm_indices: &'a [(usize, ImageFilesystemCandidate)],
    pv_offsets: &[u64],
) -> Option<&'a ImageFilesystemCandidate> {
    for offset in pv_offsets {
        if let Some((_, candidate)) = lvm_indices
            .iter()
            .find(|(_, candidate)| candidate.offset == *offset)
        {
            return Some(candidate);
        }
    }
    None
}

fn lvm_sources_for_pv_mappings(
    sources: &[LvmPhysicalVolumeSource],
    pv_mappings: &[(String, u64)],
    pv_offsets: &[u64],
) -> Vec<LvmPhysicalVolumeSource> {
    if !pv_mappings.is_empty() {
        return pv_mappings
            .iter()
            .filter_map(|(pv_name, offset)| {
                sources
                    .iter()
                    .find(|source| {
                        source.offset == *offset
                            && source.pv_name.as_deref().is_none_or(|name| name == pv_name)
                    })
                    .cloned()
                    .map(|mut source| {
                        source.pv_name = Some(pv_name.clone());
                        source
                    })
            })
            .collect();
    }

    pv_offsets
        .iter()
        .filter_map(|offset| sources.iter().find(|source| source.offset == *offset))
        .cloned()
        .collect()
}

fn remove_lvm_candidates_for_offsets(
    remove_indices: &mut Vec<usize>,
    lvm_indices: &[(usize, ImageFilesystemCandidate)],
    pv_offsets: &[u64],
) {
    for (idx, candidate) in lvm_indices {
        if pv_offsets.contains(&candidate.offset) && !remove_indices.contains(idx) {
            remove_indices.push(*idx);
        }
    }
}

fn mark_lvm_partitions_expanded(probe: &mut ImageFilesystemProbe, pv_offsets: &[u64]) {
    for partition in &mut probe.partitions {
        if pv_offsets.contains(&partition.offset)
            && matches!(partition.filesystem, Some(ImageFilesystemKind::LvmPool))
        {
            partition.status = PartitionStatus::Expanded;
        }
    }
}
