use super::collection::CollectionPlanError;
use crate::{
    datasource_service::LvmDiscoverySource,
    source_runtime::{
        self,
        content::{self, SourceContent},
    },
};
use source_kernel::resource_budget::{global_pool, ResourcePermit, Resources};
use std::{
    path::Path,
    sync::{atomic::AtomicBool, Arc},
};

pub(super) struct CollectionContent {
    identities: Vec<Arc<SourceContent>>,
    factories: Vec<evidence_core::SharedReaderFactory>,
    _permits: Arc<Vec<ResourcePermit>>,
}

impl std::fmt::Debug for CollectionContent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(&self.identities).finish()
    }
}
impl PartialEq for CollectionContent {
    fn eq(&self, other: &Self) -> bool {
        self.identities == other.identities
    }
}
impl Eq for CollectionContent {}

impl CollectionContent {
    pub(super) fn fingerprint(&self) -> String {
        use sha2::{Digest, Sha256};
        let mut digest = Sha256::new();
        digest.update(b"collection-logical-content-v1");
        let mut identities = self.identities.iter().collect::<Vec<_>>();
        identities.sort_by(|left, right| left.source_path.cmp(&right.source_path));
        for identity in identities {
            digest.update((identity.source_path.len() as u64).to_le_bytes());
            digest.update(identity.source_path.as_bytes());
            digest.update(identity.source_kind.to_string().as_bytes());
            digest.update(identity.logical_size.to_le_bytes());
            digest.update(identity.logical_sha256.as_bytes());
        }
        hex::encode(digest.finalize())
    }

    pub(super) fn capture(
        sources: &[LvmDiscoverySource],
        cancel: &AtomicBool,
    ) -> Result<Self, CollectionPlanError> {
        let mut identities = Vec::new();
        let mut permits = Vec::new();
        let mut factories = Vec::new();
        let mut total_bytes = 0u64;
        for source in sources {
            super::collection::check_cancel(cancel)?;
            let used = global_pool().snapshot().0;
            let limits = global_pool().limits();
            let (parse_bytes, parse_metadata) = if source.source_kind == domain::DataSourceKind::E01
            {
                (1024 * 1024 * 1024, 258 * 1024 * 1024)
            } else {
                let parse = source_kernel::source_runtime::resources::raw_parse_request();
                (parse.bytes, parse.metadata)
            };
            if used.bytes.saturating_add(parse_bytes) > limits.bytes
                || used.metadata.saturating_add(parse_metadata) > limits.metadata
            {
                return Err(source_kernel::resource_budget::ResourceError::Limit
                    .into_io()
                    .into());
            }
            let factory = source_runtime::open_factory_with_cancel(
                &source.source_path,
                &source.source_kind,
                cancel,
            )?;
            let bytes = SourceContent::estimated_bytes(factory.logical_size())?;
            total_bytes = total_bytes
                .checked_add(bytes)
                .filter(|bytes| *bytes <= 128 * 1024 * 1024)
                .ok_or_else(|| source_kernel::resource_budget::ResourceError::Limit.into_io())?;
            let permit = global_pool()
                .acquire(
                    Resources {
                        bytes,
                        metadata: bytes,
                        ..Default::default()
                    },
                    Some(cancel),
                )
                .map_err(source_kernel::resource_budget::ResourceError::into_io)?;
            let mut reader = factory.open_reader_with_cancel(cancel)?;
            let identity = content::capture_reader(
                &source.source_path,
                &source.source_kind,
                &mut *reader,
                cancel,
            )?;
            factory.validate_snapshot()?;
            identities.push(Arc::new(identity));
            permits.push(permit);
            factories.push(factory);
        }
        Ok(Self {
            identities,
            factories,
            _permits: Arc::new(permits),
        })
    }

    pub(super) fn get(
        &self,
        path: &Path,
        kind: &domain::DataSourceKind,
    ) -> Result<&Arc<SourceContent>, CollectionPlanError> {
        let canonical = path.canonicalize()?.to_string_lossy().into_owned();
        self.identities
            .iter()
            .find(|identity| identity.source_path == canonical && identity.source_kind == *kind)
            .ok_or(CollectionPlanError::MemberMismatch)
    }

    pub(super) fn factory(
        &self,
        path: &Path,
        kind: &domain::DataSourceKind,
        cancel: &AtomicBool,
    ) -> Result<evidence_core::SharedReaderFactory, CollectionPlanError> {
        let identity = self.get(path, kind)?;
        super::collection::check_cancel(cancel)?;
        let index = self
            .identities
            .iter()
            .position(|known| std::ptr::eq(known, identity))
            .ok_or(CollectionPlanError::MemberMismatch)?;
        let inner = self.factories[index].clone();
        Ok(content::VerifiedFactory::wrap(
            inner,
            Arc::clone(identity),
            self._permits.clone(),
        ))
    }

    pub(super) fn verify(&self, cancel: &AtomicBool) -> Result<(), CollectionPlanError> {
        for (identity, factory) in self.identities.iter().zip(&self.factories) {
            identity.verify_factory(factory, cancel)?;
        }
        Ok(())
    }

    pub(super) fn verify_sources(
        &self,
        paths: &std::collections::BTreeSet<std::path::PathBuf>,
        cancel: &AtomicBool,
    ) -> Result<(), CollectionPlanError> {
        for path in paths {
            let index = self
                .identities
                .iter()
                .position(|identity| Path::new(&identity.source_path) == path)
                .ok_or(CollectionPlanError::MemberMismatch)?;
            self.identities[index].verify_factory(&self.factories[index], cancel)?;
        }
        Ok(())
    }
}
