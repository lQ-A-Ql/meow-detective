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
    #[error("collection content verification failed: {0}")]
    Content(crate::source_runtime::content::ContentError),
    #[error("collection MD discovery failed: {0}")]
    MdRaid(#[from] fs_lvm::mdraid::MdRaidError),
}

impl From<crate::source_runtime::content::ContentError> for CollectionPlanError {
    fn from(error: crate::source_runtime::content::ContentError) -> Self {
        match error {
            crate::source_runtime::content::ContentError::Cancelled => Self::Cancelled,
            crate::source_runtime::content::ContentError::Io(error) => Self::from(error),
            error => Self::Content(error),
        }
    }
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

#[derive(Debug, PartialEq, Eq)]
pub struct ImageCollectionPlan {
    sources: Vec<LvmDiscoverySource>,
    probes: Vec<ImageFilesystemProbe>,
    shared_members: Vec<bool>,
    snapshot: SourceSnapshot,
    source_snapshots: Vec<SourceSnapshot>,
    content: super::collection_content::CollectionContent,
    md_members: Vec<crate::mdraid::MdRaidMemberPlan>,
    observations: Vec<Vec<domain::structure::NativeObservation>>,
    _memory: std::sync::Arc<super::collection_memory::PlanMemory>,
}

impl ImageCollectionPlan {
    pub fn prepare(
        sources: &[LvmDiscoverySource],
        cancel: &AtomicBool,
    ) -> Result<Self, CollectionPlanError> {
        let preparation = source_kernel::resource_budget::global_pool()
            .acquire(
                source_kernel::resource_budget::Resources {
                    bytes: super::collection_memory::PREPARATION_BYTES,
                    metadata: super::collection_memory::RETAINED_BYTES,
                    catalogs: 1,
                    ..Default::default()
                },
                Some(cancel),
            )
            .map_err(source_kernel::resource_budget::ResourceError::into_io)?;
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
        let source_snapshots = canonical
            .iter()
            .map(|source| SourceSnapshot::capture_images(std::slice::from_ref(source)))
            .collect::<Result<Vec<_>, _>>()?;
        let snapshot = SourceSnapshot::merged(&source_snapshots);
        let content = super::collection_content::CollectionContent::capture(&canonical, cancel)?;
        let (mut probes, pv_infos, mut md_members) = inspect_sources(&canonical, &content, cancel)?;
        let mut warnings = Vec::new();
        let complete = super::collection_groups::complete_groups(&pv_infos, &mut warnings)?;
        let mut shared_members = vec![false; canonical.len()];
        for group in complete {
            check_cancel(cancel)?;
            md_members.extend(expand_group(
                &canonical,
                &mut probes,
                &mut shared_members,
                group,
                &content,
            )?);
        }
        for probe in &mut probes {
            probe.warnings.extend(warnings.iter().cloned());
        }
        let observations =
            super::collection_observations::collect(&canonical, &pv_infos, &md_members)?;
        snapshot.validate()?;
        let memory = super::collection_memory::retain(
            preparation,
            &canonical,
            &probes,
            &snapshot,
            &md_members,
            &observations,
        )?;
        Ok(Self {
            sources: canonical,
            probes,
            shared_members,
            snapshot,
            source_snapshots,
            content,
            md_members,
            observations,
            _memory: memory,
        })
    }

    pub fn verify_content(&self, cancel: &AtomicBool) -> Result<(), CollectionPlanError> {
        check_cancel(cancel)?;
        self.validate_snapshot()?;
        self.content.verify(cancel)?;
        self.validate_snapshot()
    }

    pub fn content_fingerprint(&self) -> String {
        self.content.fingerprint()
    }

    pub fn native_observations(
        &self,
        path: &Path,
        kind: &domain::DataSourceKind,
    ) -> Result<&[domain::structure::NativeObservation], CollectionPlanError> {
        self.validate_snapshot()?;
        let path = path.canonicalize()?;
        let index = self
            .sources
            .iter()
            .position(|source| source.source_path == path && source.source_kind == *kind)
            .ok_or(CollectionPlanError::MemberMismatch)?;
        Ok(&self.observations[index])
    }

    pub(super) fn validate_input_identity(&self, path: &Path) -> Result<(), CollectionPlanError> {
        let source = self
            .sources
            .iter()
            .find(|source| source.source_path == path)
            .ok_or(CollectionPlanError::MemberMismatch)?;
        self.content.get(path, &source.source_kind)?.validate()?;
        Ok(())
    }

    pub(crate) fn source_snapshot(
        &self,
        sources: &[LvmDiscoverySource],
    ) -> Result<SourceSnapshot, CollectionPlanError> {
        let snapshots = sources
            .iter()
            .map(|source| {
                self.sources
                    .iter()
                    .position(|known| {
                        known.source_path == source.source_path
                            && known.source_kind == source.source_kind
                    })
                    .map(|index| self.source_snapshots[index].clone())
                    .ok_or(CollectionPlanError::MemberMismatch)
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(SourceSnapshot::merged(&snapshots))
    }

    pub fn source_content(
        &self,
        path: &Path,
        kind: &domain::DataSourceKind,
    ) -> Result<&crate::source_runtime::content::SourceContent, CollectionPlanError> {
        self.content
            .get(path, kind)
            .map(|identity| identity.as_ref())
    }

    pub fn verified_factory(
        &self,
        path: &Path,
        kind: &domain::DataSourceKind,
        cancel: &AtomicBool,
    ) -> Result<evidence_core::SharedReaderFactory, CollectionPlanError> {
        self.validate_snapshot()?;
        self.content.factory(path, kind, cancel)
    }

    pub(crate) fn md_members(&self) -> &[crate::mdraid::MdRaidMemberPlan] {
        &self.md_members
    }
    pub(super) fn contains_candidate(
        &self,
        candidate: &datasource_service::ImageFilesystemCandidate,
    ) -> bool {
        self.probes
            .iter()
            .any(|probe| probe.candidates.contains(candidate))
    }

    pub(super) fn contains_lvm_identity(
        &self,
        identity: &datasource_service::LvmLogicalVolumeIdentity,
    ) -> bool {
        self.probes.iter().any(|probe| {
            probe
                .candidates
                .iter()
                .filter_map(|item| item.lvm_identity.as_ref())
                .chain(
                    probe
                        .partitions
                        .iter()
                        .filter_map(|item| item.lvm_identity.as_ref()),
                )
                .chain(
                    probe
                        .unsupported_volumes
                        .iter()
                        .filter_map(|item| item.lvm_identity.as_ref()),
                )
                .any(|known| known == identity)
        })
    }

    pub fn validate_snapshot(&self) -> Result<(), CollectionPlanError> {
        self.content.validate_reads()?;
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

type SourceDiscovery = (
    Vec<ImageFilesystemProbe>,
    Vec<super::model::LvmPvDiscoveryInfo>,
    Vec<crate::mdraid::MdRaidMemberPlan>,
);

fn inspect_sources(
    canonical: &[LvmDiscoverySource],
    content: &super::collection_content::CollectionContent,
    cancel: &AtomicBool,
) -> Result<SourceDiscovery, CollectionPlanError> {
    let mut probes = Vec::with_capacity(canonical.len());
    let mut pv_infos = Vec::new();
    let mut md_members = Vec::new();
    for source in canonical {
        let mut uuids = HashSet::new();
        check_cancel(cancel)?;
        let factory = content.factory(&source.source_path, &source.source_kind, cancel)?;
        let mut reader = factory.open_reader_with_cancel(cancel)?;
        let probe = datasource_service::detect_image_filesystem(&mut reader)?;
        md_members.extend(crate::mdraid::discovery::direct_members(
            source, &probe, &factory,
        )?);
        for (_, candidate) in expansion::collect_lvm_candidates(&probe) {
            let info = discovery::inspect_lvm_pv_reader(
                &candidate,
                &source.source_path,
                &source.source_kind,
                &mut *reader,
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
    Ok((probes, pv_infos, md_members))
}

fn expand_group(
    sources: &[LvmDiscoverySource],
    probes: &mut [ImageFilesystemProbe],
    shared: &mut [bool],
    group: Vec<datasource_service::LvmPhysicalVolumeSource>,
    content: &super::collection_content::CollectionContent,
) -> Result<Vec<crate::mdraid::MdRaidMemberPlan>, CollectionPlanError> {
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
        Some(content),
    )?;
    let md_members = std::mem::take(&mut state.md_members);
    let expanded = !state.remove_indices.is_empty();
    expansion::apply_expansion_results(&mut probes[owner], state);
    if !expanded {
        return Ok(md_members);
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
    Ok(md_members)
}

pub(super) fn check_cancel(cancel: &AtomicBool) -> Result<(), CollectionPlanError> {
    if cancel.load(Ordering::Acquire) {
        return Err(CollectionPlanError::Cancelled);
    }
    Ok(())
}
