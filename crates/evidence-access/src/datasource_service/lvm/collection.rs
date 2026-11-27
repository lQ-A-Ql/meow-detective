use std::{
    collections::HashSet,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};

use super::{discovery, expansion, model::LvmExpansionState, source_identity};
use crate::{
    datasource_service::{self, ImageFilesystemProbe, LvmDiscoverySource},
    source_runtime::snapshot::SourceSnapshot,
};

#[derive(Debug, thiserror::Error)]
pub enum CollectionPlanError {
    #[error("collection discovery cancelled")]
    Cancelled,
    #[error("collection image access failed: {0}")]
    Io(std::io::Error),
    #[error("collection image discovery failed: {0}")]
    Probe(#[from] datasource_service::DataSourceError),
    #[error("collection LVM metadata failed: {0}")]
    Lvm(#[from] fs_lvm::LvmError),
    #[error("collection physical volume discovery failed: {0}")]
    PhysicalVolume(String),
    #[error("collection contains duplicate source or PV identities")]
    DuplicateIdentity,
    #[error("collection member does not match the shared image plan")]
    MemberMismatch,
}

impl From<std::io::Error> for CollectionPlanError {
    fn from(error: std::io::Error) -> Self {
        if error.kind() == std::io::ErrorKind::Interrupted {
            Self::Cancelled
        } else {
            Self::Io(error)
        }
    }
}

impl transport::ServiceErrorCategory for CollectionPlanError {
    fn category(&self) -> transport::ErrorCategory {
        match self {
            Self::Cancelled => transport::ErrorCategory::Cancelled,
            Self::Io(_) => transport::ErrorCategory::Io,
            Self::Probe(error) => error.category(),
            _ => transport::ErrorCategory::Validation,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageCollectionPlan {
    sources: Vec<LvmDiscoverySource>,
    probes: Vec<ImageFilesystemProbe>,
    shared_members: Vec<bool>,
    snapshot: SourceSnapshot,
}

impl ImageCollectionPlan {
    pub fn prepare(
        sources: &[LvmDiscoverySource],
        cancel: &AtomicBool,
    ) -> Result<Self, CollectionPlanError> {
        let mut unique = HashSet::new();
        let mut canonical = Vec::with_capacity(sources.len());
        for source in sources {
            check_cancel(cancel)?;
            let path = source.source_path.canonicalize()?;
            if !unique.insert(path.clone()) {
                return Err(CollectionPlanError::DuplicateIdentity);
            }
            canonical.push(LvmDiscoverySource::new(path, source.source_kind.clone()));
        }
        let snapshot = SourceSnapshot::capture_images(&canonical)?;
        let mut probes = Vec::with_capacity(canonical.len());
        let mut pv_infos = Vec::new();
        let mut uuids = HashSet::new();
        for source in &canonical {
            check_cancel(cancel)?;
            let factory = crate::source_runtime::open_factory_with_cancel(
                &source.source_path,
                &source.source_kind,
                cancel,
            )?;
            let mut reader = factory.open_reader_with_cancel(cancel)?;
            let probe = datasource_service::detect_image_filesystem(&mut reader)?;
            drop(reader);
            for (_, candidate) in expansion::collect_lvm_candidates(&probe) {
                let info = discovery::inspect_lvm_pv_candidate(
                    &candidate,
                    &source.source_path,
                    &source.source_kind,
                )
                .map_err(CollectionPlanError::PhysicalVolume)?;
                if !uuids.insert(source_identity::normalize_lvm_uuid_for_match(
                    &info.label.pv_uuid,
                )) {
                    return Err(CollectionPlanError::DuplicateIdentity);
                }
                pv_infos.push(info);
            }
            factory.validate_snapshot()?;
            probes.push(probe);
        }
        let groups = discovery::collect_metadata_groups(&pv_infos)?;
        let keys = pv_infos
            .iter()
            .map(|info| source_identity::lvm_pv_source_key(&info.source))
            .collect();
        let mut warnings = Vec::new();
        let (complete, _) =
            discovery::complete_metadata_groups(&pv_infos, &groups, &keys, &mut warnings);
        let mut shared_members = vec![false; canonical.len()];
        for group in complete {
            check_cancel(cancel)?;
            expand_group(&canonical, &mut probes, &mut shared_members, group)?;
        }
        for probe in &mut probes {
            probe.warnings.extend(warnings.iter().cloned());
        }
        snapshot.validate()?;
        Ok(Self {
            sources: canonical,
            probes,
            shared_members,
            snapshot,
        })
    }

    pub fn validate_snapshot(&self) -> Result<(), CollectionPlanError> {
        self.snapshot.validate()?;
        Ok(())
    }

    pub fn member_probe(
        &self,
        index: usize,
        path: &Path,
        kind: &domain::DataSourceKind,
    ) -> Result<ImageFilesystemProbe, CollectionPlanError> {
        self.validate_snapshot()?;
        let source = self
            .sources
            .get(index)
            .ok_or(CollectionPlanError::MemberMismatch)?;
        if source.source_kind != *kind || source.source_path != path.canonicalize()? {
            return Err(CollectionPlanError::MemberMismatch);
        }
        Ok(self.probes[index].clone())
    }

    pub fn is_shared_member(&self, index: usize) -> bool {
        self.shared_members.get(index).copied().unwrap_or(false)
    }
}

fn expand_group(
    sources: &[LvmDiscoverySource],
    probes: &mut [ImageFilesystemProbe],
    shared: &mut [bool],
    group: Vec<datasource_service::LvmPhysicalVolumeSource>,
) -> Result<(), CollectionPlanError> {
    let members = sources
        .iter()
        .enumerate()
        .filter(|(_, source)| {
            group.iter().any(|pv| {
                source_identity::lvm_source_matches(pv, &source.source_path, &source.source_kind)
            })
        })
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    let owner = members
        .iter()
        .copied()
        .min_by_key(|index| sources[*index].source_path.clone())
        .ok_or(CollectionPlanError::MemberMismatch)?;
    let source = &sources[owner];
    let indices = expansion::collect_lvm_candidates(&probes[owner]);
    let mut state = LvmExpansionState::default();
    expansion::expand_discovery_group(
        &mut probes[owner],
        &indices,
        &source.source_path,
        &source.source_kind,
        group.clone(),
        &mut state,
    )?;
    let expanded = !state.remove_indices.is_empty();
    expansion::apply_expansion_results(&mut probes[owner], state);
    if !expanded {
        return Ok(());
    }
    for member in members {
        if member == owner {
            continue;
        }
        let source = &sources[member];
        let offsets = group
            .iter()
            .filter(|pv| {
                source_identity::lvm_source_matches(pv, &source.source_path, &source.source_kind)
            })
            .map(|pv| pv.offset)
            .collect::<Vec<_>>();
        expansion::mark_lvm_partitions_expanded(&mut probes[member], &offsets);
        probes[member].candidates.retain(|candidate| {
            candidate.kind != datasource_service::ImageFilesystemKind::LvmPool
                || !offsets.contains(&candidate.offset)
        });
        shared[member] = true;
    }
    Ok(())
}

fn check_cancel(cancel: &AtomicBool) -> Result<(), CollectionPlanError> {
    if cancel.load(Ordering::Acquire) {
        return Err(CollectionPlanError::Cancelled);
    }
    Ok(())
}
