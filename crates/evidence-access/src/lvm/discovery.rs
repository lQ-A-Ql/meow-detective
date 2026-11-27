use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::datasource_service::{
    ImageCollectionPlan, ImageFilesystemCandidate, LvmDiscoverySource,
};

use super::plan::{
    lvm_volume_fingerprint, segment_digest, LvmPlanError, LvmVolumePlan, LvmVolumeSourcePlan,
    LVM_ADAPTER_VERSION, LVM_POLICY_VERSION,
};

pub fn discover_volumes_from_collection(
    collection: &ImageCollectionPlan,
    cancel: &AtomicBool,
) -> Result<Vec<LvmVolumePlan>, LvmPlanError> {
    if cancel.load(Ordering::Acquire) {
        return Err(LvmPlanError::Cancelled);
    }
    collection.validate_snapshot()?;
    let mut by_vg = BTreeMap::<String, Vec<&ImageFilesystemCandidate>>::new();
    for candidate in collection.logical_volume_candidates() {
        let identity = candidate
            .lvm_identity
            .as_ref()
            .ok_or(LvmPlanError::InvalidContentIdentity)?;
        by_vg
            .entry(identity.vg_uuid.clone())
            .or_default()
            .push(candidate);
    }
    let mut plans = Vec::new();
    for candidates in by_vg.values() {
        discover_group(collection, candidates, cancel, &mut plans)?;
    }
    plans.sort_by(|left, right| {
        (&left.vg_uuid, &left.lv_uuid).cmp(&(&right.vg_uuid, &right.lv_uuid))
    });
    if plans.is_empty() {
        return Ok(plans);
    }
    for plan in &plans {
        lvm_volume_fingerprint(plan)?;
    }
    collection.validate_snapshot()?;
    Ok(plans)
}

fn discover_group(
    collection: &ImageCollectionPlan,
    candidates: &[&ImageFilesystemCandidate],
    cancel: &AtomicBool,
    plans: &mut Vec<LvmVolumePlan>,
) -> Result<(), LvmPlanError> {
    let identity = candidates
        .first()
        .and_then(|candidate| candidate.lvm_identity.as_ref())
        .ok_or(LvmPlanError::InvalidContentIdentity)?;
    let discovery_sources = unique_sources(identity)?;
    let snapshot = collection.source_snapshot(&discovery_sources)?;
    let mut readers = Vec::new();
    for source in &discovery_sources {
        if cancel.load(Ordering::Acquire) {
            return Err(LvmPlanError::Cancelled);
        }
        let factory =
            collection.verified_factory(&source.source_path, &source.source_kind, cancel)?;
        readers.push(factory.open_reader_with_cancel(cancel)?);
    }
    let offsets = identity.pv_offsets.clone();
    let pool = fs_lvm::LvmPool::discover(readers, offsets)?;
    let volume_group = pool.volume_group();
    if normalize(&volume_group.id) != normalize(&identity.vg_uuid) {
        return Err(LvmPlanError::SourceContentChanged);
    }
    for candidate in candidates {
        let identity = candidate
            .lvm_identity
            .as_ref()
            .ok_or(LvmPlanError::InvalidContentIdentity)?;
        let volume = volume_group
            .logical_volumes
            .iter()
            .find(|volume| normalize(&volume.uuid) == normalize(&identity.lv_uuid))
            .ok_or(LvmPlanError::SourceContentChanged)?;
        if candidate.length != Some(volume.size_bytes) {
            return Err(LvmPlanError::InvalidContentIdentity);
        }
        let sources = identity
            .pv_sources
            .iter()
            .map(|source| {
                let kind = source
                    .source_kind
                    .clone()
                    .ok_or(LvmPlanError::InvalidContentIdentity)?;
                let content = collection
                    .source_content(Path::new(&source.source_path), &kind)?
                    .clone();
                Ok(LvmVolumeSourcePlan {
                    source_path: source.source_path.clone(),
                    source_kind: kind,
                    pv_uuid: source.pv_uuid.clone(),
                    offset: source.offset,
                    content,
                })
            })
            .collect::<Result<Vec<_>, LvmPlanError>>()?;
        let plan = LvmVolumePlan {
            schema_version: LvmVolumePlan::schema_version(),
            vg_uuid: volume_group.id.clone(),
            vg_name: volume_group.name.clone(),
            vg_seqno: volume_group.seqno,
            vg_extent_size_sectors: volume_group.extent_size,
            lv_uuid: volume.uuid.clone(),
            lv_name: volume.name.clone(),
            logical_size: volume.size_bytes,
            segment_sha256: segment_digest(volume_group, volume)?,
            sources,
            source_snapshot: snapshot.clone(),
            adapter_version: LVM_ADAPTER_VERSION.into(),
            policy_version: LVM_POLICY_VERSION.into(),
        };
        plan.validate()?;
        plans.push(plan);
    }
    Ok(())
}

fn unique_sources(
    identity: &crate::datasource_service::LvmLogicalVolumeIdentity,
) -> Result<Vec<LvmDiscoverySource>, LvmPlanError> {
    let mut seen = BTreeSet::new();
    let mut sources = Vec::new();
    for source in &identity.pv_sources {
        let kind = source
            .source_kind
            .clone()
            .ok_or(LvmPlanError::InvalidContentIdentity)?;
        let path = std::fs::canonicalize(&source.source_path)?;
        if seen.insert((path.clone(), kind.to_string())) {
            sources.push(LvmDiscoverySource::new(path, kind));
        }
    }
    if sources.is_empty() {
        return Err(LvmPlanError::InvalidContentIdentity);
    }
    Ok(sources)
}

fn normalize(uuid: &str) -> String {
    crate::datasource_service::normalize_lvm_uuid_for_match(uuid)
}
