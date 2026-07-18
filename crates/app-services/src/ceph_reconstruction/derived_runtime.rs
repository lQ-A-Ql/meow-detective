use std::path::Path;

use domain::{CaseId, DataSourceId};
use persistence_sqlite::repositories::ceph_rbd_lineage_repo::CephRbdLineageAggregate;
use sha2::{Digest, Sha256};

use super::{
    derived_reader::{build_replica_bindings, descriptor_from_lineage, load_lineage},
    open_rbd_head_image, BluestoreDeviceOpener, DerivedRbdReaderError, RbdEvidenceReader,
    RbdImageDescriptor, SharedRadosObjectProvider, SourceBoundLvmError,
    SourceDbRadosObjectProvider, STRICT_RBD_REPLICA_COUNT,
};

#[derive(Clone)]
pub struct DerivedRbdRuntime {
    data_source_id: DataSourceId,
    lineage_fingerprint: String,
    descriptor: RbdImageDescriptor,
    provider: SharedRadosObjectProvider,
    cache_capacity_bytes: usize,
}

struct PreviewBluestoreDeviceOpener {
    case_id: String,
}

impl BluestoreDeviceOpener for PreviewBluestoreDeviceOpener {
    fn open(
        &self,
        source_connection: &rusqlite::Connection,
        data_source_id: &DataSourceId,
        inventory_id: &str,
    ) -> Result<Box<dyn evidence_core::EvidenceReader>, SourceBoundLvmError> {
        super::source_bound_lvm::open_source_bound_bluestore_lvm_for_case(
            source_connection,
            data_source_id,
            inventory_id,
            &self.case_id,
        )
    }
}

impl DerivedRbdRuntime {
    pub fn data_source_id(&self) -> &DataSourceId {
        &self.data_source_id
    }

    pub fn lineage_fingerprint(&self) -> &str {
        &self.lineage_fingerprint
    }

    pub fn descriptor(&self) -> &RbdImageDescriptor {
        &self.descriptor
    }

    pub fn cache_capacity_bytes(&self) -> usize {
        self.cache_capacity_bytes
    }

    pub fn read_metrics(&self) -> super::RadosProviderReadMetrics {
        self.provider.read_metrics()
    }

    pub fn open_reader(&self) -> Result<RbdEvidenceReader, DerivedRbdReaderError> {
        open_rbd_head_image(&self.descriptor, Box::new(self.provider.clone()))
            .map_err(|error| DerivedRbdReaderError::Open(error.to_string()))
    }
}

pub fn build_derived_rbd_runtime(
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    case_id: &CaseId,
    derived_data_source_id: &DataSourceId,
) -> Result<DerivedRbdRuntime, DerivedRbdReaderError> {
    let aggregate = load_lineage(case_conn, derived_data_source_id)?;
    if aggregate.lineage.expected_replica_count as usize != STRICT_RBD_REPLICA_COUNT
        || aggregate.replicas.len() != STRICT_RBD_REPLICA_COUNT
    {
        return Err(DerivedRbdReaderError::Provider(format!(
            "RBD lineage requires exactly {STRICT_RBD_REPLICA_COUNT} replicas"
        )));
    }
    let replicas = build_replica_bindings(case_conn, case_root, case_id, &aggregate)?;
    let descriptor = descriptor_from_lineage(&aggregate);
    let provider = SourceDbRadosObjectProvider::with_device_opener(
        replicas,
        descriptor.metadata.data_pool_id,
        Vec::new(),
        aggregate.lineage.expected_replica_count as usize,
        Box::new(PreviewBluestoreDeviceOpener {
            case_id: case_id.0.clone(),
        }),
    )
    .map_err(|error| DerivedRbdReaderError::Provider(error.to_string()))?;
    let cache_capacity_bytes = provider.cache_capacity_bytes();

    Ok(DerivedRbdRuntime {
        data_source_id: derived_data_source_id.clone(),
        lineage_fingerprint: lineage_fingerprint(&aggregate),
        descriptor,
        provider: SharedRadosObjectProvider::new(provider),
        cache_capacity_bytes,
    })
}

pub fn load_lineage_fingerprint(
    case_conn: &rusqlite::Connection,
    derived_data_source_id: &DataSourceId,
) -> Result<String, DerivedRbdReaderError> {
    load_lineage(case_conn, derived_data_source_id).map(|aggregate| lineage_fingerprint(&aggregate))
}

fn lineage_fingerprint(aggregate: &CephRbdLineageAggregate) -> String {
    let lineage = &aggregate.lineage;
    let mut hasher = Sha256::new();
    for value in [
        lineage.derived_data_source_id.as_str(),
        lineage.parent_cluster_id.as_str(),
        lineage.image_name.as_str(),
        lineage.image_id.as_str(),
        lineage.object_prefix.as_str(),
        lineage.scope_identity.as_str(),
    ] {
        update_fingerprint_field(&mut hasher, value.as_bytes());
    }
    for value in [
        lineage.image_size,
        u64::from(lineage.object_order),
        lineage.features,
        lineage.stripe_unit,
        lineage.stripe_count,
        lineage.data_pool_id as u64,
        lineage.operation_features,
        lineage.snapshot_id.unwrap_or(u64::MAX),
        u64::from(lineage.expected_replica_count),
    ] {
        update_fingerprint_field(&mut hasher, &value.to_le_bytes());
    }
    update_fingerprint_field(&mut hasher, &[u8::from(lineage.has_parent)]);
    update_fingerprint_field(&mut hasher, &[u8::from(lineage.encrypted)]);

    for replica in &aggregate.replicas {
        update_fingerprint_field(&mut hasher, &replica.ordinal.to_le_bytes());
        update_fingerprint_field(&mut hasher, replica.source_data_source_id.as_bytes());
        update_fingerprint_field(&mut hasher, replica.inventory_id.as_bytes());
        update_fingerprint_field(&mut hasher, &replica.osd_id.to_le_bytes());
    }
    hex::encode(hasher.finalize())
}

fn update_fingerprint_field(hasher: &mut Sha256, value: &[u8]) {
    hasher.update((value.len() as u64).to_le_bytes());
    hasher.update(value);
}

#[cfg(test)]
#[path = "../../tests/unit/ceph_reconstruction/derived_runtime.rs"]
mod tests;
