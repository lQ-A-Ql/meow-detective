use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;

use domain::DataSourceId;
use evidence_core::EvidenceReader;
use persistence_sqlite::repositories::ceph_bluestore_semantic_repo::{
    CephBluestoreObjectCandidate, CephBluestoreObjectReadPlan, CephBluestoreSemanticRepo,
};
use thiserror::Error;

use super::{
    open_source_bound_bluestore_lvm, RadosObjectReader, RbdObjectProvider, RbdObjectProviderError,
    RbdObjectReadOutcome, RbdObjectReadRequest, SourceBoundLvmError,
};

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
    replicas: Vec<RadosReplicaSource>,
    pool: i64,
    namespace: Vec<u8>,
    expected_replica_count: usize,
    device_opener: Box<dyn BluestoreDeviceOpener>,
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
            replicas,
            pool,
            namespace,
            expected_replica_count,
            device_opener,
        })
    }

    fn read_replica(
        &self,
        replica: &RadosReplicaSource,
        request: &RbdObjectReadRequest,
    ) -> Result<Option<Vec<u8>>, RadosProviderError> {
        let connection = persistence_sqlite::open_existing_source(&replica.source_db_path)
            .map_err(|_| RadosProviderError::SourceDb {
                inventory_id: replica.inventory_id.clone(),
                detail: "source database could not be opened".to_string(),
            })?;
        let repo = CephBluestoreSemanticRepo::new(&connection);
        let Some(candidate) = repo
            .find_object_candidate(
                &replica.inventory_id,
                request.object_identity.as_bytes(),
                self.pool,
                &self.namespace,
            )
            .map_err(|error| RadosProviderError::ObjectLookup {
                inventory_id: replica.inventory_id.clone(),
                detail: error.to_string(),
            })?
        else {
            return Ok(None);
        };
        let plan = repo
            .find_object_read_plan(&replica.inventory_id, &candidate.object_identity_sha256)
            .map_err(|error| RadosProviderError::ObjectLookup {
                inventory_id: replica.inventory_id.clone(),
                detail: error.to_string(),
            })?
            .ok_or_else(|| RadosProviderError::ReadPlanMissing {
                inventory_id: replica.inventory_id.clone(),
            })?;
        validate_candidate(&candidate, request)?;
        let device = self
            .device_opener
            .open(&connection, &replica.data_source_id, &replica.inventory_id)
            .map_err(|error| RadosProviderError::DeviceUnavailable {
                inventory_id: replica.inventory_id.clone(),
                detail: error.to_string(),
            })?;
        read_plan_range(device, plan, request)
            .map(Some)
            .map_err(|error| RadosProviderError::ObjectRead {
                inventory_id: replica.inventory_id.clone(),
                detail: error.to_string(),
            })
    }
}

impl RbdObjectProvider for SourceDbRadosObjectProvider {
    fn read_object_range(
        &mut self,
        request: &RbdObjectReadRequest,
        output: &mut [u8],
    ) -> Result<RbdObjectReadOutcome, RbdObjectProviderError> {
        if output.len() != request.length {
            return Err(RbdObjectProviderError::ReadFailed {
                object_identity: request.object_identity.clone(),
                reason: "provider output length does not match request".to_string(),
            });
        }
        if self.replicas.len() != self.expected_replica_count {
            return Err(RbdObjectProviderError::Unavailable {
                object_identity: request.object_identity.clone(),
                reason: "RBD replica coverage is no longer closed".to_string(),
            });
        }

        let mut expected: Option<Vec<u8>> = None;
        for replica in &self.replicas {
            let Some(bytes) = self.read_replica(replica, request).map_err(|source| {
                RbdObjectProviderError::ReadFailed {
                    object_identity: request.object_identity.clone(),
                    reason: source.to_string(),
                }
            })?
            else {
                continue;
            };
            if let Some(reference) = &expected {
                if reference != &bytes {
                    return Err(RbdObjectProviderError::ReadFailed {
                        object_identity: request.object_identity.clone(),
                        reason: "RBD replicas returned conflicting bytes".to_string(),
                    });
                }
            } else {
                expected = Some(bytes);
            }
        }

        let Some(bytes) = expected else {
            return Ok(RbdObjectReadOutcome::Missing);
        };
        output.copy_from_slice(&bytes);
        Ok(RbdObjectReadOutcome::Present {
            object_identity: request.object_identity.clone(),
            bytes_read: output.len(),
        })
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
    Ok(())
}

fn read_plan_range(
    device: Box<dyn EvidenceReader>,
    plan: CephBluestoreObjectReadPlan,
    request: &RbdObjectReadRequest,
) -> std::io::Result<Vec<u8>> {
    let mut reader =
        RadosObjectReader::new(device, plan).map_err(|source| io_error(source.to_string()))?;
    reader.seek(SeekFrom::Start(request.object_offset))?;
    let mut bytes = vec![0; request.length];
    reader.read_exact(&mut bytes)?;
    Ok(bytes)
}

fn io_error(message: String) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, message)
}

#[cfg(test)]
#[path = "../../tests/unit/ceph_reconstruction/rados_provider.rs"]
mod tests;
