use std::path::PathBuf;

use domain::{
    CaseId, DataSource, DataSourceId, DataSourceKind, DataSourcePlatform, DataSourceProvenance,
    DataSourceProvenanceStatus,
};
use persistence_sqlite::repositories::{
    ceph_rbd_lineage_repo::{
        CephRbdLineageAggregate, CephRbdLineageRecord, CephRbdLineageRepo, CephRbdReplicaRecord,
    },
    datasource_repo::{DataSourceRepo, DataSourceStorage},
};

use crate::ceph_reconstruction::{
    load_lineage_fingerprint, RbdImageDescriptor, STRICT_RBD_REPLICA_COUNT,
};

use super::{DerivedSourceError, DerivedSourceResult};

pub(super) fn validate_existing_registration(
    case_conn: &rusqlite::Connection,
    cluster_id: &str,
    existing_source: &DataSource,
    desired_source: &DataSource,
    descriptor: &RbdImageDescriptor,
    replica_records: &[CephRbdReplicaRecord],
) -> DerivedSourceResult<String> {
    if existing_source.kind != DataSourceKind::CephRbd
        || existing_source.source_path != desired_source.source_path
    {
        return Err(DerivedSourceError::InconsistentState(format!(
            "derived source {} registration does not match the requested RBD image",
            existing_source.id.0
        )));
    }
    let expected = lineage_aggregate(&existing_source.id, cluster_id, descriptor, replica_records);
    let stored = CephRbdLineageRepo::new(case_conn)
        .find_by_data_source(&existing_source.id.0)?
        .ok_or_else(|| {
            DerivedSourceError::InconsistentState(format!(
                "derived source {} is missing its RBD lineage",
                existing_source.id.0
            ))
        })?;
    if stored != expected {
        return Err(DerivedSourceError::InconsistentState(format!(
            "derived source {} RBD lineage changed and cannot reuse the existing registration",
            existing_source.id.0
        )));
    }
    load_lineage_fingerprint(case_conn, &existing_source.id)
        .map_err(|error| DerivedSourceError::Reconstruction(error.to_string()))
}

pub(super) fn build_data_source(
    cluster_id: &str,
    data_source_id: &DataSourceId,
    descriptor: &RbdImageDescriptor,
) -> DataSource {
    DataSource {
        id: data_source_id.clone(),
        name: descriptor.metadata.name.clone(),
        kind: DataSourceKind::CephRbd,
        source_path: PathBuf::from(format!(
            "ceph-rbd://{cluster_id}/{}",
            descriptor.metadata.id
        )),
        imported_at: chrono::Utc::now(),
        provenance: DataSourceProvenance {
            source_hash_sha256: None,
            hash_status: domain::DataSourceHashStatus::Unavailable,
            canonical_source_path: None,
            evidence_size: Some(descriptor.metadata.image_size),
            reader_kind: Some("ceph-rbd".to_string()),
            provenance_status: DataSourceProvenanceStatus::Recorded,
            warnings: Vec::new(),
        },
    }
}

pub(super) fn register_derived_source(
    case_conn: &rusqlite::Connection,
    case_id: &CaseId,
    cluster_id: &str,
    data_source: &DataSource,
    descriptor: &RbdImageDescriptor,
    replica_records: &[CephRbdReplicaRecord],
) -> DerivedSourceResult<String> {
    let storage = DataSourceStorage::source_db(
        &data_source.id.0,
        Some(DataSourcePlatform::Linux.as_storage_str()),
        Some("vm_disk".to_string()),
    );
    let lineage = lineage_aggregate(&data_source.id, cluster_id, descriptor, replica_records);
    persistence_sqlite::repositories::ceph_rbd_lineage_repo::validate_aggregate(&lineage)?;
    let transaction = case_conn
        .unchecked_transaction()
        .map_err(persistence_sqlite::DbError::from)?;
    DataSourceRepo::new(&transaction).insert_with_storage(case_id, data_source, &storage)?;
    persistence_sqlite::repositories::ceph_rbd_lineage_repo::insert_aggregate_in_transaction(
        &transaction,
        &lineage,
    )?;
    transaction
        .commit()
        .map_err(persistence_sqlite::DbError::from)?;
    load_lineage_fingerprint(case_conn, &data_source.id)
        .map_err(|error| DerivedSourceError::Reconstruction(error.to_string()))
}

pub(super) fn lineage_aggregate(
    data_source_id: &DataSourceId,
    cluster_id: &str,
    descriptor: &RbdImageDescriptor,
    replicas: &[CephRbdReplicaRecord],
) -> CephRbdLineageAggregate {
    let metadata = &descriptor.metadata;
    CephRbdLineageAggregate {
        lineage: CephRbdLineageRecord {
            derived_data_source_id: data_source_id.0.clone(),
            parent_cluster_id: cluster_id.to_string(),
            image_name: metadata.name.clone(),
            image_id: metadata.id.clone(),
            object_prefix: metadata.object_prefix.clone(),
            image_size: metadata.image_size,
            object_order: metadata.order,
            features: metadata.features,
            stripe_unit: metadata.stripe_unit,
            stripe_count: metadata.stripe_count,
            data_pool_id: metadata.data_pool_id,
            scope_identity: descriptor.scope_identity.clone(),
            operation_features: descriptor.context.operation_features,
            has_parent: descriptor.context.has_parent,
            snapshot_id: descriptor.context.snapshot_id,
            encrypted: descriptor.context.encrypted,
            expected_replica_count: STRICT_RBD_REPLICA_COUNT as u32,
        },
        replicas: replicas.to_vec(),
    }
}
