use super::collection::{CollectionPlanError, ImageCollectionPlan};
use crate::datasource_service::{
    normalize_lvm_uuid_for_match, ImageFilesystemCandidate, LvmLogicalVolumeIdentity,
};
use evidence_core::{
    EvidenceReader, EvidenceReaderFactory, ReaderCapabilities, SharedReaderFactory,
};
use std::{
    io,
    path::Path,
    sync::{atomic::AtomicBool, Arc},
};

impl ImageCollectionPlan {
    pub fn open_verified_lvm_reader(
        &self,
        identity: &LvmLogicalVolumeIdentity,
        scope: Option<Arc<dyn evidence_core::reader::ReaderAllocationScope>>,
    ) -> Result<Box<dyn EvidenceReader>, CollectionPlanError> {
        if !self.contains_lvm_identity(identity) {
            return Err(CollectionPlanError::MemberMismatch);
        }
        self.validate_snapshot()?;
        let route = crate::source_runtime::resources::ResourceRoute::default();
        if let Some(scope) = scope {
            route.bind(scope);
        }
        Ok(open_volume(self, identity, &route)?)
    }

    pub fn logical_volume_factory(
        self: &Arc<Self>,
        candidate: &ImageFilesystemCandidate,
    ) -> Result<SharedReaderFactory, CollectionPlanError> {
        if !self.contains_candidate(candidate) || candidate.lvm_identity.is_none() {
            return Err(CollectionPlanError::MemberMismatch);
        }
        Ok(Arc::new(LogicalFactory {
            plan: Arc::clone(self),
            candidate: candidate.clone(),
            route: crate::source_runtime::resources::ResourceRoute::default(),
        }))
    }
}

struct LogicalFactory {
    plan: Arc<ImageCollectionPlan>,
    candidate: ImageFilesystemCandidate,
    route: crate::source_runtime::resources::ResourceRoute,
}

impl EvidenceReaderFactory for LogicalFactory {
    fn bind_reader_scope(&self, scope: Arc<dyn evidence_core::reader::ReaderAllocationScope>) {
        self.route.bind(scope);
    }
    fn reader_scope(&self) -> Option<Arc<dyn evidence_core::reader::ReaderAllocationScope>> {
        self.route.scope()
    }
    fn open_reader(&self) -> io::Result<Box<dyn EvidenceReader>> {
        self.plan.validate_snapshot().map_err(io::Error::other)?;
        let identity = self
            .candidate
            .lvm_identity
            .as_ref()
            .ok_or_else(|| io::Error::other("logical identity missing"))?;
        open_volume(&self.plan, identity, &self.route)
    }
    fn logical_size(&self) -> u64 {
        self.candidate.length.unwrap_or(0)
    }
    fn preferred_read_granularity(&self) -> usize {
        512
    }
    fn capabilities(&self) -> ReaderCapabilities {
        ReaderCapabilities {
            max_concurrent_readers: 1,
            ..Default::default()
        }
    }
    fn validate_snapshot(&self) -> io::Result<()> {
        self.plan.validate_snapshot().map_err(io::Error::other)
    }
}

fn open_volume(
    plan: &ImageCollectionPlan,
    identity: &LvmLogicalVolumeIdentity,
    route: &crate::source_runtime::resources::ResourceRoute,
) -> io::Result<Box<dyn EvidenceReader>> {
    let pool = crate::lvm::mapping_cache::open(
        &identity.pv_sources,
        |source| {
            let kind = source
                .source_kind
                .as_ref()
                .ok_or_else(|| io::Error::other("PV source kind missing"))?;
            plan.source_content(Path::new(&source.source_path), kind)
                .cloned()
                .map_err(io::Error::other)
        },
        &AtomicBool::new(false),
        route.scope(),
    )?;
    if normalize_lvm_uuid_for_match(&pool.volume_group().id)
        != normalize_lvm_uuid_for_match(&identity.vg_uuid)
    {
        return Err(io::Error::other("logical volume group identity changed"));
    }
    let index = pool
        .list_volumes()
        .iter()
        .position(|volume| volume.uuid == identity.lv_uuid)
        .ok_or_else(|| io::Error::other("logical volume identity changed"))?;
    pool.open_volume(index)
}
