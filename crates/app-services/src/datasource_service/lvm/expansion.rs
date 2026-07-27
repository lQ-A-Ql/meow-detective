use super::super::fs_magic::{kind_label, read_boot_filesystem};
use super::super::reader::open_evidence_reader;
use super::super::{
    ImageFilesystemCandidate, ImageFilesystemKind, ImageFilesystemProbe, ImageFilesystemSource,
    LvmDiscoverySource, LvmLogicalVolumeIdentity, LvmPhysicalVolumeSource, PartitionRecord,
    PartitionStatus,
};
use super::diagnostics::{
    format_lvm_lv_summaries, format_lvm_pv_sources, lvm_lv_diagnostic_context,
    lvm_vg_diagnostic_context,
};
use super::discovery::lvm_discovery_pv_groups;
use super::model::{ExpandedPoolSources, LvmExpansionState};
use super::source_identity::{
    lvm_source_fingerprint, lvm_source_matches, lvm_sources_for_pv_mappings,
    representative_lvm_candidate,
};
use super::unsupported::classify_unsupported_logical_volume;
use domain::DataSourceKind;
use std::collections::BTreeSet;
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
    let lvm_indices = collect_lvm_candidates(probe);
    if lvm_indices.is_empty() {
        return;
    }

    let discovery_groups = lvm_discovery_pv_groups(
        &lvm_indices,
        source_path,
        source_kind,
        extra_sources,
        &mut probe.warnings,
    );
    let mut state = LvmExpansionState::default();
    for pv_sources in discovery_groups {
        expand_discovery_group(
            probe,
            &lvm_indices,
            source_path,
            source_kind,
            pv_sources,
            &mut state,
        );
    }
    apply_expansion_results(probe, state);
}

fn collect_lvm_candidates(probe: &ImageFilesystemProbe) -> Vec<(usize, ImageFilesystemCandidate)> {
    probe
        .candidates
        .iter()
        .enumerate()
        .filter(|(_, candidate)| matches!(candidate.kind, ImageFilesystemKind::LvmPool))
        .map(|(index, candidate)| (index, candidate.clone()))
        .collect()
}

fn expand_discovery_group(
    probe: &mut ImageFilesystemProbe,
    lvm_indices: &[(usize, ImageFilesystemCandidate)],
    source_path: &Path,
    source_kind: &DataSourceKind,
    pv_sources: Vec<LvmPhysicalVolumeSource>,
    state: &mut LvmExpansionState,
) {
    let Some((pool, seed_offset)) = discover_lvm_pool(probe, &pv_sources, source_kind) else {
        return;
    };
    let expanded = expanded_pool_sources(&pool, &pv_sources, source_path, source_kind, seed_offset);
    let representative = representative_lvm_candidate(
        lvm_indices,
        if expanded.primary_offsets.is_empty() {
            &expanded.offsets
        } else {
            &expanded.primary_offsets
        },
    )
    .or_else(|| {
        lvm_indices
            .iter()
            .find(|(_, candidate)| candidate.offset == expanded.seed_offset)
            .map(|(_, candidate)| candidate)
    });

    let vg = pool.volume_group();
    let vg_key = if vg.id.is_empty() {
        vg.name.clone()
    } else {
        vg.id.clone()
    };
    if !state.expanded_vgs.insert(vg_key) {
        redirect_expanded_pool(probe, lvm_indices, &expanded.primary_offsets, state);
        return;
    }

    let lv_list = pool.list_volumes();
    trace_discovered_volumes(lv_list.len(), &expanded);
    let readable_volumes = pool.list_readable_volumes();
    warn_unsupported_volumes(probe, vg, &lv_list, &readable_volumes, &expanded.sources);
    let candidates_before = state.new_candidates.len();
    append_readable_volume_candidates(
        probe,
        &pool,
        readable_volumes,
        representative,
        &expanded,
        state,
    );
    redirect_expanded_pool(probe, lvm_indices, &expanded.primary_offsets, state);
    if state.new_candidates.len() == candidates_before {
        probe.warnings.push(format!(
            "LVM expand: volume group produced no supported logical volume candidates; {} LV candidate(s)={}",
            lvm_vg_diagnostic_context(vg, &expanded.sources),
            format_lvm_lv_summaries(vg)
        ));
    }
}

fn discover_lvm_pool(
    probe: &mut ImageFilesystemProbe,
    pv_sources: &[LvmPhysicalVolumeSource],
    default_source_kind: &DataSourceKind,
) -> Option<(fs_lvm::LvmPool, u64)> {
    let pv_offsets = pv_sources
        .iter()
        .map(|source| source.offset)
        .collect::<Vec<_>>();
    let seed_offset = pv_offsets.first().copied().unwrap_or_default();
    let mut readers = Vec::with_capacity(pv_sources.len());
    for pv_source in pv_sources {
        let reader_path = Path::new(&pv_source.source_path);
        let reader_kind = pv_source
            .source_kind
            .as_ref()
            .unwrap_or(default_source_kind);
        match open_evidence_reader(reader_path, reader_kind) {
            Ok(reader) => readers.push(reader),
            Err(error) => {
                warn_reader_open_failure(probe, pv_source, error);
                return None;
            }
        }
    }
    if readers.is_empty() {
        return None;
    }

    match fs_lvm::LvmPool::discover(readers, pv_offsets) {
        Ok(pool) => Some((pool, seed_offset)),
        Err(error) => {
            let formatted_sources = format_lvm_pv_sources(pv_sources);
            probe.warnings.push(format!(
                "LVM expand: discovery failed for PV source(s) {}: {}",
                formatted_sources, error
            ));
            tracing::warn!(
                "LVM expand: discovery failed for PV source(s) {}: {}",
                formatted_sources,
                error
            );
            None
        }
    }
}

fn warn_reader_open_failure(
    probe: &mut ImageFilesystemProbe,
    pv_source: &LvmPhysicalVolumeSource,
    error: impl std::fmt::Display,
) {
    probe.warnings.push(format!(
        "LVM expand: cannot open reader for PV source='{}' offset {}: {}",
        lvm_source_fingerprint(&pv_source.source_path),
        pv_source.offset,
        error
    ));
    tracing::warn!(
        "LVM expand: cannot open reader for PV source='{}' at offset {}: {}",
        lvm_source_fingerprint(&pv_source.source_path),
        pv_source.offset,
        error
    );
}

fn expanded_pool_sources(
    pool: &fs_lvm::LvmPool,
    pv_sources: &[LvmPhysicalVolumeSource],
    source_path: &Path,
    source_kind: &DataSourceKind,
    seed_offset: u64,
) -> ExpandedPoolSources {
    let pv_mappings = pool
        .physical_volume_offsets()
        .iter()
        .map(|(pv_name, offset)| (pv_name.clone(), *offset))
        .collect::<Vec<_>>();
    let mapped_offsets = if pv_mappings.is_empty() {
        vec![seed_offset]
    } else {
        pv_mappings.iter().map(|(_, offset)| *offset).collect()
    };
    let mut sources = lvm_sources_for_pv_mappings(pv_sources, &pv_mappings, &mapped_offsets);
    if sources.len() != pv_sources.len() {
        sources = pv_sources.to_vec();
    }

    let offsets = sources
        .iter()
        .map(|source| source.offset)
        .collect::<Vec<_>>();
    let primary_offsets = sources
        .iter()
        .filter(|source| lvm_source_matches(source, source_path, source_kind))
        .map(|source| source.offset)
        .collect::<Vec<_>>();
    let candidate_offset = primary_offsets
        .first()
        .copied()
        .or_else(|| offsets.first().copied())
        .unwrap_or(seed_offset);
    ExpandedPoolSources {
        sources,
        offsets,
        primary_offsets,
        candidate_offset,
        seed_offset,
    }
}

fn trace_discovered_volumes(volume_count: usize, expanded: &ExpandedPoolSources) {
    tracing::info!(
        "LVM: {} logical volume(s) discovered at offset {}",
        volume_count,
        expanded
            .offsets
            .first()
            .copied()
            .unwrap_or(expanded.seed_offset),
    );
}

fn warn_unsupported_volumes(
    probe: &mut ImageFilesystemProbe,
    volume_group: &fs_lvm::VolumeGroup,
    lv_list: &[fs_lvm::LvInfo],
    readable_volumes: &[(usize, fs_lvm::LvInfo)],
    expanded_sources: &[LvmPhysicalVolumeSource],
) {
    let readable_indices = readable_volumes
        .iter()
        .map(|(index, _)| *index)
        .collect::<BTreeSet<_>>();
    for (lv_idx, lv_info) in lv_list.iter().enumerate() {
        if readable_indices.contains(&lv_idx) {
            continue;
        }
        let reason = lv_info
            .unsupported_reason
            .as_deref()
            .unwrap_or("unsupported logical volume mapping");
        probe.warnings.push(format!(
            "LVM expand: skipping unsupported logical volume; {}: {}",
            lvm_lv_diagnostic_context(volume_group, lv_info, expanded_sources),
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

fn append_readable_volume_candidates(
    probe: &mut ImageFilesystemProbe,
    pool: &fs_lvm::LvmPool,
    readable_volumes: Vec<(usize, fs_lvm::LvInfo)>,
    representative: Option<&ImageFilesystemCandidate>,
    expanded: &ExpandedPoolSources,
    state: &mut LvmExpansionState,
) {
    let vg = pool.volume_group();
    for (lv_idx, lv_info) in readable_volumes {
        let mut lv_reader = match pool.open_volume_reader(lv_idx) {
            Ok(reader) => reader,
            Err(error) => {
                probe.warnings.push(format!(
                    "LVM expand: open logical volume failed; {}: {}",
                    lvm_lv_diagnostic_context(vg, &lv_info, &expanded.sources),
                    error
                ));
                tracing::warn!("LVM: open_volume '{}' failed: {}", lv_info.name, error);
                continue;
            }
        };

        let filesystem = read_boot_filesystem(&mut *lv_reader, 0);
        match filesystem {
            Ok(Some(fs_kind)) if !matches!(fs_kind, ImageFilesystemKind::LvmPool) => {
                state.new_candidates.push(build_logical_volume_candidate(
                    representative,
                    vg,
                    &lv_info,
                    fs_kind,
                    expanded,
                ));
            }
            Ok(_) => {
                classify_unsupported_logical_volume(probe, &mut *lv_reader, vg, &lv_info, expanded);
            }
            Err(error) => {
                probe.warnings.push(format!(
                    "LVM expand: filesystem detection failed for logical volume; {}: {}",
                    lvm_lv_diagnostic_context(vg, &lv_info, &expanded.sources),
                    error
                ));
                tracing::debug!("LVM LV '{}': FS detection error: {}", lv_info.name, error);
            }
        }
    }
}

fn build_logical_volume_candidate(
    representative: Option<&ImageFilesystemCandidate>,
    vg: &fs_lvm::VolumeGroup,
    lv_info: &fs_lvm::LvInfo,
    fs_kind: ImageFilesystemKind,
    expanded: &ExpandedPoolSources,
) -> (ImageFilesystemCandidate, u64) {
    let vg_name = if vg.name.is_empty() {
        representative
            .and_then(|candidate| candidate.partition_name.as_deref())
            .unwrap_or("LVM")
    } else {
        vg.name.as_str()
    };
    let lvm_identity = LvmLogicalVolumeIdentity {
        vg_uuid: vg.id.clone(),
        vg_name: vg.name.clone(),
        lv_uuid: lv_info.uuid.clone(),
        lv_name: lv_info.name.clone(),
        pv_offsets: expanded.offsets.clone(),
        pv_sources: expanded.sources.clone(),
    };
    (
        ImageFilesystemCandidate {
            partition_index: representative.and_then(|candidate| candidate.partition_index),
            partition_name: Some(format!("{vg_name}/{}", lv_info.name)),
            kind: fs_kind,
            offset: expanded.candidate_offset,
            length: Some(lv_info.size_bytes),
            source: ImageFilesystemSource::LvmLogicalVolume,
            lvm_identity: Some(lvm_identity),
        },
        lv_info.size_bytes,
    )
}

fn redirect_expanded_pool(
    probe: &mut ImageFilesystemProbe,
    lvm_indices: &[(usize, ImageFilesystemCandidate)],
    primary_offsets: &[u64],
    state: &mut LvmExpansionState,
) {
    mark_lvm_partitions_expanded(probe, primary_offsets);
    remove_lvm_candidates_for_offsets(&mut state.remove_indices, lvm_indices, primary_offsets);
}

fn apply_expansion_results(probe: &mut ImageFilesystemProbe, mut state: LvmExpansionState) {
    state.remove_indices.sort_unstable_by(|a, b| b.cmp(a));
    for index in state.remove_indices {
        probe.candidates.remove(index);
    }

    let next_index = probe
        .partitions
        .iter()
        .map(|partition| partition.index)
        .max()
        .unwrap_or(0)
        + 1;
    for (position, (candidate, size_bytes)) in state.new_candidates.iter_mut().enumerate() {
        append_logical_volume_partition(probe, candidate, *size_bytes, next_index + position);
    }
    probe.candidates.extend(
        state
            .new_candidates
            .into_iter()
            .map(|(candidate, _)| candidate),
    );
}

fn append_logical_volume_partition(
    probe: &mut ImageFilesystemProbe,
    candidate: &mut ImageFilesystemCandidate,
    size_bytes: u64,
    index: usize,
) {
    candidate.partition_index = Some(index);
    probe.partitions.push(PartitionRecord {
        index,
        name: candidate
            .partition_name
            .clone()
            .unwrap_or_else(|| format!("LV_{index}")),
        kind_label: kind_label(candidate.kind),
        type_guid: None,
        offset: candidate.offset,
        length: size_bytes,
        status: PartitionStatus::Supported,
        filesystem: Some(candidate.kind),
        lvm_identity: candidate.lvm_identity.clone(),
    });
}

fn remove_lvm_candidates_for_offsets(
    remove_indices: &mut Vec<usize>,
    lvm_indices: &[(usize, ImageFilesystemCandidate)],
    pv_offsets: &[u64],
) {
    for (index, candidate) in lvm_indices {
        if pv_offsets.contains(&candidate.offset) && !remove_indices.contains(index) {
            remove_indices.push(*index);
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
