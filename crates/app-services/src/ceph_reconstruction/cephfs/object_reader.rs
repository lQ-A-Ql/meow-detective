mod range;
mod types;
mod validation;

use std::sync::Arc;

use persistence_sqlite::repositories::{
    ceph_bluestore_semantic_repo::CephBluestoreReadPlanSession,
    ceph_fs_metadata_inventory_repo::CephFsMetadataInventoryRepo,
};

use super::{CephFsDescriptor, CephFsObjectLocator};
use crate::ceph_reconstruction::{
    rados_provider::SharedEvidenceReader, BluestoreDeviceOpener, FilesystemBluestoreDeviceOpener,
    RadosObjectReader,
};
use validation::{
    checked_range, validate_descriptor, validate_locator, validate_manifest, validate_plan,
    validate_replica_metadata, validate_sources,
};

pub use types::{
    CephFsObjectRange, CephFsObjectReadError, CephFsObjectReadProvenance, CephFsObjectSource,
    MAX_CEPHFS_OBJECT_RANGE_LENGTH,
};

pub struct SourceDbCephFsObjectReader {
    descriptor: CephFsDescriptor,
    sources: Vec<SourceRuntime>,
    expected_replica_count: usize,
    device_opener: Box<dyn BluestoreDeviceOpener>,
}

struct SourceRuntime {
    binding: CephFsObjectSource,
    session: Option<CephBluestoreReadPlanSession>,
    device: Option<SharedEvidenceReader>,
}

pub(super) struct ResolvedReplica {
    provenance: CephFsObjectReadProvenance,
    record_sha256: String,
    object_size: u64,
    device: SharedEvidenceReader,
    layout: Arc<super::super::rados_reader::RadosObjectLayout>,
}

impl SourceDbCephFsObjectReader {
    pub fn new(
        descriptor: CephFsDescriptor,
        sources: Vec<CephFsObjectSource>,
        expected_replica_count: usize,
    ) -> Result<Self, CephFsObjectReadError> {
        Self::with_device_opener(
            descriptor,
            sources,
            expected_replica_count,
            Box::new(FilesystemBluestoreDeviceOpener),
        )
    }

    pub fn with_device_opener(
        descriptor: CephFsDescriptor,
        mut sources: Vec<CephFsObjectSource>,
        expected_replica_count: usize,
        device_opener: Box<dyn BluestoreDeviceOpener>,
    ) -> Result<Self, CephFsObjectReadError> {
        validate_descriptor(&descriptor)?;
        validate_sources(&descriptor, &sources, expected_replica_count)?;
        sources.sort_by(|left, right| {
            (&left.data_source_id.0, &left.inventory_id)
                .cmp(&(&right.data_source_id.0, &right.inventory_id))
        });
        Ok(Self {
            descriptor,
            sources: sources
                .into_iter()
                .map(|binding| SourceRuntime {
                    binding,
                    session: None,
                    device: None,
                })
                .collect(),
            expected_replica_count,
            device_opener,
        })
    }

    pub fn read_range(
        &mut self,
        locator: &CephFsObjectLocator,
        offset: u64,
        length: usize,
    ) -> Result<CephFsObjectRange, CephFsObjectReadError> {
        validate_locator(&self.descriptor, locator)?;
        if length > MAX_CEPHFS_OBJECT_RANGE_LENGTH {
            return Err(CephFsObjectReadError::RangeTooLarge {
                requested: length,
                maximum: MAX_CEPHFS_OBJECT_RANGE_LENGTH,
            });
        }
        let canonical = locator.canonical();
        let replicas = self.resolve_replicas(&canonical, locator)?;
        let object_size = validate_replica_metadata(&canonical, &replicas)?;
        checked_range(locator, offset, length, object_size)?;
        let bytes = range::read_and_verify(&canonical, offset, length, &replicas)?;
        Ok(CephFsObjectRange {
            filesystem_identity: self.descriptor.identity.clone(),
            locator: canonical,
            object_size,
            offset,
            bytes,
            provenance: replicas
                .into_iter()
                .map(|replica| replica.provenance)
                .collect(),
        })
    }

    fn resolve_replicas(
        &mut self,
        canonical: &str,
        locator: &CephFsObjectLocator,
    ) -> Result<Vec<ResolvedReplica>, CephFsObjectReadError> {
        let mut replicas = Vec::new();
        for index in 0..self.sources.len() {
            if let Some(replica) = self.resolve_source(index, canonical, locator)? {
                replicas.push(replica);
            }
        }
        if replicas.is_empty() {
            return Err(CephFsObjectReadError::ObjectNotFound {
                locator: canonical.to_string(),
            });
        }
        if replicas.len() != self.expected_replica_count {
            return Err(CephFsObjectReadError::ReplicaCoverageIncomplete {
                locator: canonical.to_string(),
                expected: self.expected_replica_count,
                present: replicas.len(),
            });
        }
        Ok(replicas)
    }

    fn resolve_source(
        &mut self,
        index: usize,
        canonical: &str,
        locator: &CephFsObjectLocator,
    ) -> Result<Option<ResolvedReplica>, CephFsObjectReadError> {
        let runtime = &mut self.sources[index];
        ensure_session(runtime)?;
        let session =
            runtime
                .session
                .as_ref()
                .ok_or_else(|| CephFsObjectReadError::SourceDbUnavailable {
                    inventory_id: runtime.binding.inventory_id.clone(),
                })?;
        let repo = CephFsMetadataInventoryRepo::new(session.connection());
        let manifest = repo
            .find_manifest(&self.descriptor.identity, &runtime.binding.inventory_id)
            .map_err(|_| inventory_unavailable(runtime))?
            .ok_or_else(|| inventory_unavailable(runtime))?;
        validate_manifest(
            &self.descriptor,
            &runtime.binding,
            session.semantic_sha256(),
            &manifest,
        )?;
        let Some(projection) = repo
            .find_object_by_locator(
                &self.descriptor.identity,
                &runtime.binding.inventory_id,
                canonical,
            )
            .map_err(|_| inventory_unavailable(runtime))?
        else {
            return Ok(None);
        };
        let plan = session
            .find_object_read_plan(&projection.object_identity_sha256)
            .map_err(|_| read_plan_unavailable(runtime))?
            .ok_or_else(|| read_plan_unavailable(runtime))?;
        validate_plan(locator, &projection, &plan)?;
        let layout =
            RadosObjectReader::prepare_layout(&plan).map_err(|_| read_plan_unavailable(runtime))?;
        if runtime.device.is_none() {
            let device = self
                .device_opener
                .open(
                    session.connection(),
                    &runtime.binding.data_source_id,
                    &runtime.binding.inventory_id,
                )
                .map_err(|_| CephFsObjectReadError::DeviceUnavailable {
                    inventory_id: runtime.binding.inventory_id.clone(),
                })?;
            runtime.device = Some(SharedEvidenceReader::new(device));
        }
        let device =
            runtime
                .device
                .clone()
                .ok_or_else(|| CephFsObjectReadError::DeviceUnavailable {
                    inventory_id: runtime.binding.inventory_id.clone(),
                })?;
        Ok(Some(ResolvedReplica {
            provenance: CephFsObjectReadProvenance {
                data_source_id: runtime.binding.data_source_id.0.clone(),
                inventory_id: runtime.binding.inventory_id.clone(),
                object_identity_sha256: projection.object_identity_sha256,
            },
            record_sha256: projection.record_sha256,
            object_size: plan.object.size,
            device,
            layout,
        }))
    }
}

fn ensure_session(runtime: &mut SourceRuntime) -> Result<(), CephFsObjectReadError> {
    if runtime.session.is_some() {
        return Ok(());
    }
    let connection =
        persistence_sqlite::open_existing_source_read_only(&runtime.binding.source_db_path)
            .map_err(|_| CephFsObjectReadError::SourceDbUnavailable {
                inventory_id: runtime.binding.inventory_id.clone(),
            })?;
    runtime.session = Some(
        CephBluestoreReadPlanSession::new(connection, &runtime.binding.inventory_id).map_err(
            |_| CephFsObjectReadError::SourceDbUnavailable {
                inventory_id: runtime.binding.inventory_id.clone(),
            },
        )?,
    );
    Ok(())
}

fn inventory_unavailable(runtime: &SourceRuntime) -> CephFsObjectReadError {
    CephFsObjectReadError::InventoryUnavailable {
        inventory_id: runtime.binding.inventory_id.clone(),
    }
}

fn read_plan_unavailable(runtime: &SourceRuntime) -> CephFsObjectReadError {
    CephFsObjectReadError::ReadPlanUnavailable {
        inventory_id: runtime.binding.inventory_id.clone(),
    }
}
