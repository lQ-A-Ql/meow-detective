use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use ceph_wire::RBD_HEAD_SNAP_HEX;
use domain::DataSourceId;
use evidence_core::EvidenceReader;
use persistence_sqlite::repositories::ceph_bluestore_semantic_repo::{
    CephBluestoreObjectReadPlan, CephBluestoreReadPlanSession,
};
use thiserror::Error;

use super::rados_reader::{RadosObjectLayout, RadosObjectReader};
use super::{
    open_source_bound_bluestore_lvm, RbdObjectProvider, RbdObjectProviderError,
    RbdObjectReadOutcome, RbdObjectReadRequest, SourceBoundLvmError, STRICT_RBD_REPLICA_COUNT,
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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RadosProviderReadMetrics {
    pub verified_cache_hits: u64,
    pub verified_cache_misses: u64,
    pub plan_cache_hits: u64,
    pub plan_cache_misses: u64,
    pub plan_lookup_elapsed_micros: u64,
    pub read_plan_session_initializations: u64,
    pub read_plan_session_elapsed_micros: u64,
    pub replica_device_reads: u64,
    pub replica_device_bytes: u64,
    pub replica_device_elapsed_micros: u64,
}

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
    read_metrics: RadosProviderReadMetrics,
}

struct ReplicaRuntime {
    binding: RadosReplicaSource,
    read_plan_session: Option<CephBluestoreReadPlanSession>,
    device: Option<SharedEvidenceReader>,
    plans: ObjectPlanCache,
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
        if expected_replica_count != STRICT_RBD_REPLICA_COUNT
            || replicas.len() != STRICT_RBD_REPLICA_COUNT
        {
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
                    read_plan_session: None,
                    device: None,
                    plans: ObjectPlanCache::for_rbd(),
                })
                .collect(),
            pool,
            namespace,
            expected_replica_count,
            device_opener,
            verified_objects: VerifiedObjectCache::for_rbd(),
            read_metrics: RadosProviderReadMetrics::default(),
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
        if let Some(elapsed_micros) = ensure_replica_read_plan_session(runtime, &replica)? {
            self.read_metrics.read_plan_session_initializations = self
                .read_metrics
                .read_plan_session_initializations
                .saturating_add(1);
            self.read_metrics.read_plan_session_elapsed_micros = self
                .read_metrics
                .read_plan_session_elapsed_micros
                .saturating_add(elapsed_micros);
        }
        let session =
            runtime
                .read_plan_session
                .as_ref()
                .ok_or_else(|| RadosProviderError::SourceDb {
                    inventory_id: replica.inventory_id.clone(),
                    detail: "source database read-plan session was not initialized".to_string(),
                })?;
        let plan = if let Some(plan) = runtime.plans.get(&request.object_identity) {
            self.read_metrics.plan_cache_hits = self.read_metrics.plan_cache_hits.saturating_add(1);
            plan
        } else {
            self.read_metrics.plan_cache_misses =
                self.read_metrics.plan_cache_misses.saturating_add(1);
            let lookup_started = Instant::now();
            let Some(plan) = session
                .find_object_read_plan_by_name(
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
                return Ok(None);
            };
            validate_plan_binding(&plan, request)?;
            let layout = RadosObjectReader::prepare_layout(&plan).map_err(|error| {
                RadosProviderError::ObjectRead {
                    inventory_id: replica.inventory_id.clone(),
                    detail: error.to_string(),
                }
            })?;
            runtime
                .plans
                .insert(request.object_identity.clone(), layout.clone());
            self.read_metrics.plan_lookup_elapsed_micros = self
                .read_metrics
                .plan_lookup_elapsed_micros
                .saturating_add(elapsed_micros(lookup_started));
            layout
        };
        if runtime.device.is_none() {
            let device = device_opener
                .open(
                    session.connection(),
                    &replica.data_source_id,
                    &replica.inventory_id,
                )
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

fn elapsed_micros(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX)
}

fn ensure_replica_read_plan_session(
    runtime: &mut ReplicaRuntime,
    replica: &RadosReplicaSource,
) -> Result<Option<u64>, RadosProviderError> {
    if runtime.read_plan_session.is_some() {
        return Ok(None);
    }
    let started = Instant::now();
    let connection = persistence_sqlite::open_existing_source_read_only(&replica.source_db_path)
        .map_err(|error| RadosProviderError::SourceDb {
            inventory_id: replica.inventory_id.clone(),
            detail: format!(
                "source database could not be opened: {}",
                source_db_error_detail(&error)
            ),
        })?;
    runtime.read_plan_session = Some(
        CephBluestoreReadPlanSession::new(connection, &replica.inventory_id).map_err(|error| {
            RadosProviderError::SourceDb {
                inventory_id: replica.inventory_id.clone(),
                detail: format!(
                    "source database read context is invalid: {}",
                    source_db_error_detail(&error)
                ),
            }
        })?,
    );
    Ok(Some(elapsed_micros(started)))
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

fn validate_plan_binding(
    plan: &CephBluestoreObjectReadPlan,
    request: &RbdObjectReadRequest,
) -> Result<(), RadosProviderError> {
    if plan.object.object_name != request.object_identity.as_bytes() {
        return Err(RadosProviderError::ObjectLookup {
            inventory_id: plan.inventory_id.clone(),
            detail: "object lookup returned a different canonical object name".to_string(),
        });
    }
    if plan.object.snap_hex != RBD_HEAD_SNAP_HEX {
        return Err(RadosProviderError::ObjectLookup {
            inventory_id: plan.inventory_id.clone(),
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
