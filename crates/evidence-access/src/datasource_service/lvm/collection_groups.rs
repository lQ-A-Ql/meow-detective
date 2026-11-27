use super::{
    collection::CollectionPlanError, discovery, model::LvmPvDiscoveryInfo, source_identity,
};
use crate::datasource_service::LvmPhysicalVolumeSource;
use std::collections::{BTreeMap, HashSet};

pub(super) fn complete_groups(
    infos: &[LvmPvDiscoveryInfo],
    warnings: &mut Vec<String>,
) -> Result<Vec<Vec<LvmPhysicalVolumeSource>>, CollectionPlanError> {
    let mut local = BTreeMap::<String, Vec<LvmPvDiscoveryInfo>>::new();
    for info in infos {
        local
            .entry(info.source.source_path.clone())
            .or_default()
            .push(info.clone());
    }
    let mut completed = Vec::new();
    let mut assigned = HashSet::new();
    for group in local.values() {
        let metadata = discovery::collect_metadata_groups(group)?;
        let keys = group
            .iter()
            .map(|info| source_identity::lvm_pv_source_key(&info.source))
            .collect();
        let (groups, _) =
            discovery::complete_metadata_groups(group, &metadata, &keys, &mut Vec::new());
        for sources in groups {
            assigned.extend(sources.iter().map(source_identity::lvm_pv_source_key));
            completed.push(sources);
        }
    }
    let remaining = infos
        .iter()
        .filter(|info| !assigned.contains(&source_identity::lvm_pv_source_key(&info.source)))
        .cloned()
        .collect::<Vec<_>>();
    let mut uuids = HashSet::new();
    for info in &remaining {
        if !uuids.insert(source_identity::normalize_lvm_uuid_for_match(
            &info.label.pv_uuid,
        )) {
            return Err(CollectionPlanError::DuplicateIdentity);
        }
    }
    let metadata = discovery::collect_metadata_groups(&remaining)?;
    let keys = remaining
        .iter()
        .map(|info| source_identity::lvm_pv_source_key(&info.source))
        .collect();
    let (groups, _) = discovery::complete_metadata_groups(&remaining, &metadata, &keys, warnings);
    completed.extend(groups);
    Ok(completed)
}
