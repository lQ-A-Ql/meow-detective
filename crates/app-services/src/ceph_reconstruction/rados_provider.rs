use std::path::PathBuf;
use std::sync::Arc;

use ceph_wire::RBD_HEAD_SNAP_HEX;
use domain::DataSourceId;
use evidence_core::EvidenceReader;
use persistence_sqlite::repositories::ceph_bluestore_semantic_repo::{
    CephBluestoreObjectCandidate, CephBluestoreSemanticRepo,
};
use thiserror::Error;

use super::rados_reader::{RadosObjectLayout, RadosObjectReader};
use super::{
    open_source_bound_bluestore_lvm, RbdObjectProvider, RbdObjectProviderError,
    RbdObjectReadOutcome, RbdObjectReadRequest, SourceBoundLvmError,
};

mod cache;
mod device_io;
mod plan_cache;
mod range;
mod shared;

use cache::VerifiedObjectCache;
use device_io::SharedEvidenceReader;
use plan_cache::ObjectPlanCache;
pub(crate) use shared::SharedRadosObjectProvider;

/// A source-local BlueStore inventory that may contain one RBD object replica.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RadosReplicaSource {
    pub data_source_id: DataSourceId,
    pub inventory_id: String,
    pub source_db_path: PathBuf,
}

impl RadosReplicaSource {
    pub fn new(
        data_source_id: DataSourceId,
        inventory_id: impl Into<String>,
        source_db_path: impl Into<PathBuf>,
    ) -> Result<Self, RadosProviderError> {
        let inventory_id = inventory_id.into();
        let source_db_path = source_db_path.into();
        if data_source_id.0.trim().is_empty()
            || inventory_id.trim().is_empty()
            || source_db_path.as_os_str().is_empty()
        {
            return Err(RadosProviderError::InvalidReplicaBinding);
        }
        Ok(Self {
            data_source_id,
            inventory_id,
            source_db_path,
        })
    }
}

/// Reads RBD objects from a closed set of source-local BlueStore databases.
///
/// The expected replica count must come from imported cluster membership, not
/// from a directory scan. A missing object is treated as a sparse hole only
/// after the provider has verified that the supplied source set is complete.
pub struct SourceDbRadosObjectProvider {
    replicas: Vec<ReplicaRuntime>,
    pool: i64,
    namespace: Vec<u8>,
    expected_replica_count: usize,
    device_opener: Box<dyn BluestoreDeviceOpener>,
    verified_objects: VerifiedObjectCache,
}

struct ReplicaRuntime {
    binding: RadosReplicaSource,
    connection: Option<rusqlite::Connection>,
    device: Option<SharedEvidenceReader>,
    plans: ObjectPlanCache,
    catalog_complete: bool,
}

impl SourceDbRadosObjectProvider {
    pub fn new(
        replicas: Vec<RadosReplicaSource>,
        pool: i64,
        namespace: Vec<u8>,
        expected_replica_count: usize,
    ) -> Result<Self, RadosProviderError> {
        Self::with_device_opener(
            replicas,
            pool,
            namespace,
            expected_replica_count,
            Box::new(FilesystemBluestoreDeviceOpener),
        )
    }

    pub fn with_device_opener(
        replicas: Vec<RadosReplicaSource>,
        pool: i64,
        namespace: Vec<u8>,
        expected_replica_count: usize,
        device_opener: Box<dyn BluestoreDeviceOpener>,
    ) -> Result<Self, RadosProviderError> {
        if replicas.is_empty() || expected_replica_count == 0 {
            return Err(RadosProviderError::CoverageNotClosed);
        }
        if replicas.len() != expected_replica_count {
            return Err(RadosProviderError::CoverageNotClosed);
        }
        let mut identities = std::collections::HashSet::new();
        let mut data_sources = std::collections::HashSet::new();
        for replica in &replicas {
            if !identities.insert(replica.inventory_id.as_str()) {
                return Err(RadosProviderError::DuplicateInventory {
                    inventory_id: replica.inventory_id.clone(),
                });
            }
            if !data_sources.insert(replica.data_source_id.0.as_str()) {
                return Err(RadosProviderError::DuplicateSource {
                    data_source_id: replica.data_source_id.0.clone(),
                });
            }
        }
        Ok(Self {
            replicas: replicas
                .into_iter()
                .map(|binding| ReplicaRuntime {
                    binding,
                    connection: None,
                    device: None,
                    plans: ObjectPlanCache::for_rbd(),
                    catalog_complete: false,
                })
                .collect(),
            pool,
            namespace,
            expected_replica_count,
            device_opener,
            verified_objects: VerifiedObjectCache::for_rbd(),
        })
    }

    pub(crate) fn cache_capacity_bytes(&self) -> usize {
        cache::MAX_BYTES.saturating_add(
            self.replicas
                .len()
                .saturating_mul(plan_cache::MAX_PLAN_BYTES),
        )
    }

    fn resolve_replica_object(
        &mut self,
        replica_index: usize,
        request: &RbdObjectReadRequest,
    ) -> Result<Option<(SharedEvidenceReader, Arc<RadosObjectLayout>)>, RadosProviderError> {
        let device_opener = &self.device_opener;
        let runtime = &mut self.replicas[replica_index];
        let replica = runtime.binding.clone();
        if runtime.connection.is_none() {
            runtime.connection = Some(
                persistence_sqlite::open_existing_source_read_only(&replica.source_db_path)
                    .map_err(|error| RadosProviderError::SourceDb {
                        inventory_id: replica.inventory_id.clone(),
                        detail: format!(
                            "source database could not be opened: {}",
                            source_db_error_detail(&error)
                        ),
                    })?,
            );
        }
        let connection =
            runtime
                .connection
                .as_ref()
                .ok_or_else(|| RadosProviderError::SourceDb {
                    inventory_id: replica.inventory_id.clone(),
                    detail: "source database cache was not initialized".to_string(),
                })?;
        let repo = CephBluestoreSemanticRepo::new(connection);
        let plan = if let Some(plan) = runtime.plans.get(&request.object_identity) {
            plan
        } else {
            let Some(candidate) = repo
                .find_object_candidate(
                    &replica.inventory_id,
                    request.object_identity.as_bytes(),
                    self.pool,
                    &self.namespace,
                    RBD_HEAD_SNAP_HEX,
                )
                .map_err(|error| RadosProviderError::ObjectLookup {
                    inventory_id: replica.inventory_id.clone(),
                    detail: error.to_string(),
                })?
            else {
                if !runtime.catalog_complete {
                    repo.ensure_object_catalog_complete(&replica.inventory_id)
                        .map_err(|error| RadosProviderError::ObjectLookup {
                            inventory_id: replica.inventory_id.clone(),
                            detail: format!("RBD object absence is not authoritative: {error}"),
                        })?;
                    runtime.catalog_complete = true;
                }
                return Ok(None);
            };
            validate_candidate(&candidate, request)?;
            let plan = repo
                .find_object_read_plan(&replica.inventory_id, &candidate.object_identity_sha256)
                .map_err(|error| RadosProviderError::ObjectLookup {
                    inventory_id: replica.inventory_id.clone(),
                    detail: error.to_string(),
                })?
                .ok_or_else(|| RadosProviderError::ReadPlanMissing {
                    inventory_id: replica.inventory_id.clone(),
                })?;
            let layout = RadosObjectReader::prepare_layout(&plan).map_err(|error| {
                RadosProviderError::ObjectRead {
                    inventory_id: replica.inventory_id.clone(),
                    detail: error.to_string(),
                }
            })?;
            runtime
                .plans
                .insert(request.object_identity.clone(), layout.clone());
            layout
        };
        if runtime.device.is_none() {
            let device = device_opener
                .open(connection, &replica.data_source_id, &replica.inventory_id)
                .map_err(|error| RadosProviderError::DeviceUnavailable {
                    inventory_id: replica.inventory_id.clone(),
                    detail: error.to_string(),
                })?;
            runtime.device = Some(SharedEvidenceReader::new(device));
        }
        let device = runtime.device.as_ref().cloned().ok_or_else(|| {
            RadosProviderError::DeviceUnavailable {
                inventory_id: replica.inventory_id.clone(),
                detail: "source-bound device cache was not initialized".to_string(),
            }
        })?;
        Ok(Some((device, plan)))
    }

    fn validate_replica_presence(
        &self,
        request: &RbdObjectReadRequest,
        present_count: usize,
    ) -> Result<(), RadosProviderError> {
        if present_count != 0 && present_count != self.expected_replica_count {
            return Err(RadosProviderError::ObjectRead {
                inventory_id: "replica-set".to_string(),
                detail: format!(
                    "RBD replica presence is incomplete for {}: expected={}, present={present_count}",
                    request.object_identity, self.expected_replica_count
                ),
            });
        }
        Ok(())
    }
}

pub trait BluestoreDeviceOpener: Send {
    fn open(
        &self,
        source_connection: &rusqlite::Connection,
        data_source_id: &DataSourceId,
        inventory_id: &str,
    ) -> Result<Box<dyn EvidenceReader>, SourceBoundLvmError>;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct FilesystemBluestoreDeviceOpener;

impl BluestoreDeviceOpener for FilesystemBluestoreDeviceOpener {
    fn open(
        &self,
        source_connection: &rusqlite::Connection,
        data_source_id: &DataSourceId,
        inventory_id: &str,
    ) -> Result<Box<dyn EvidenceReader>, SourceBoundLvmError> {
        open_source_bound_bluestore_lvm(source_connection, data_source_id, inventory_id)
    }
}

#[derive(Debug, Error)]
pub enum RadosProviderError {
    #[error("invalid RBD replica binding")]
    InvalidReplicaBinding,
    #[error("RBD replica coverage is not closed")]
    CoverageNotClosed,
    #[error("duplicate RBD inventory binding: {inventory_id}")]
    DuplicateInventory { inventory_id: String },
    #[error("duplicate RBD data source binding: {data_source_id}")]
    DuplicateSource { data_source_id: String },
    #[error("source database unavailable for inventory {inventory_id}: {detail}")]
    SourceDb {
        inventory_id: String,
        detail: String,
    },
    #[error("RBD object lookup failed for inventory {inventory_id}: {detail}")]
    ObjectLookup {
        inventory_id: String,
        detail: String,
    },
    #[error("RBD object read plan is missing for inventory {inventory_id}")]
    ReadPlanMissing { inventory_id: String },
    #[error("RBD device is unavailable for inventory {inventory_id}: {detail}")]
    DeviceUnavailable {
        inventory_id: String,
        detail: String,
    },
    #[error("RBD object read failed for inventory {inventory_id}: {detail}")]
    ObjectRead {
        inventory_id: String,
        detail: String,
    },
}

fn validate_candidate(
    candidate: &CephBluestoreObjectCandidate,
    request: &RbdObjectReadRequest,
) -> Result<(), RadosProviderError> {
    if candidate.object_name != request.object_identity.as_bytes() {
        return Err(RadosProviderError::ObjectLookup {
            inventory_id: candidate.inventory_id.clone(),
            detail: "object lookup returned a different canonical object name".to_string(),
        });
    }
    if candidate.snap_hex != RBD_HEAD_SNAP_HEX {
        return Err(RadosProviderError::ObjectLookup {
            inventory_id: candidate.inventory_id.clone(),
            detail: "object lookup returned a non-head snapshot".to_string(),
        });
    }
    Ok(())
}

fn source_db_error_detail(error: &persistence_sqlite::DbError) -> String {
    match error {
        persistence_sqlite::DbError::Sqlite(error) => format!("sqlite error: {error}"),
        persistence_sqlite::DbError::Io(error) => {
            format!("io error of kind {:?}", error.kind())
        }
        persistence_sqlite::DbError::Migration(error) => format!("migration error: {error}"),
        persistence_sqlite::DbError::System(error) => format!("system error: {error}"),
    }
}

#[cfg(test)]
#[path = "../../tests/unit/ceph_reconstruction/rados_provider.rs"]
mod tests;
