use std::path::Path;

use domain::{CaseId, DataSourceId};
use persistence_sqlite::repositories::{
    ceph_osd_repo::CephOsdRepo,
    ceph_rbd_lineage_repo::{CephRbdLineageAggregate, CephRbdLineageRepo},
};
use thiserror::Error;

use super::{build_derived_rbd_runtime, RadosReplicaSource, RbdEvidenceReader, RbdImageDescriptor};

#[derive(Debug, Error)]
pub enum DerivedRbdReaderError {
    #[error("derived RBD lineage was not found for data source {0}")]
    LineageNotFound(String),
    #[error("derived RBD lineage is invalid: {0}")]
    Lineage(#[from] persistence_sqlite::DbError),
    #[error("RBD parent source route failed: {0}")]
    ParentSourceRoute(#[from] crate::source_db::ReadySourceError),
    #[error("RBD replica inventory {inventory_id} is not registered by source {data_source_id}")]
    InventoryMismatch {
        data_source_id: String,
        inventory_id: String,
    },
    #[error("RBD replica provider failed: {0}")]
    Provider(String),
    #[error("RBD image could not be opened: {0}")]
    Open(String),
}

pub fn open_derived_rbd_reader(
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    case_id: &CaseId,
    derived_data_source_id: &DataSourceId,
) -> Result<RbdEvidenceReader, DerivedRbdReaderError> {
    build_derived_rbd_runtime(case_conn, case_root, case_id, derived_data_source_id)?.open_reader()
}

pub(super) fn load_lineage(
    case_conn: &rusqlite::Connection,
    derived_data_source_id: &DataSourceId,
) -> Result<CephRbdLineageAggregate, DerivedRbdReaderError> {
    CephRbdLineageRepo::new(case_conn)
        .find_by_data_source(&derived_data_source_id.0)?
        .ok_or_else(|| DerivedRbdReaderError::LineageNotFound(derived_data_source_id.0.clone()))
}

pub(super) fn build_replica_bindings(
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    case_id: &CaseId,
    aggregate: &CephRbdLineageAggregate,
) -> Result<Vec<RadosReplicaSource>, DerivedRbdReaderError> {
    aggregate
        .replicas
        .iter()
        .map(|replica| {
            let source_id = DataSourceId(replica.source_data_source_id.clone());
            let source = crate::source_db::open_reconstruction_source_by_id(
                case_conn, case_root, case_id, &source_id,
            )?;
            let source_db_path =
                crate::source_db::registered_source_db_path(case_conn, case_root, &source_id)
                    .map_err(DerivedRbdReaderError::Lineage)?;
            let inventories = CephOsdRepo::new(&source.connection)
                .find_by_data_source(&source_id.0)
                .map_err(DerivedRbdReaderError::Lineage)?;
            if !inventories.iter().any(|inventory| {
                inventory.id == replica.inventory_id && inventory.whoami == Some(replica.osd_id)
            }) {
                return Err(DerivedRbdReaderError::InventoryMismatch {
                    data_source_id: replica.source_data_source_id.clone(),
                    inventory_id: replica.inventory_id.clone(),
                });
            }

            RadosReplicaSource::new(source_id, replica.inventory_id.clone(), source_db_path)
                .map_err(|error| DerivedRbdReaderError::Provider(error.to_string()))
        })
        .collect()
}

pub(super) fn descriptor_from_lineage(aggregate: &CephRbdLineageAggregate) -> RbdImageDescriptor {
    let lineage = &aggregate.lineage;
    RbdImageDescriptor {
        metadata: ceph_wire::RbdImageMetadata {
            name: lineage.image_name.clone(),
            id: lineage.image_id.clone(),
            object_prefix: lineage.object_prefix.clone(),
            image_size: lineage.image_size,
            order: lineage.object_order,
            features: lineage.features,
            stripe_unit: lineage.stripe_unit,
            stripe_count: lineage.stripe_count,
            data_pool_id: lineage.data_pool_id,
        },
        scope_identity: lineage.scope_identity.clone(),
        context: super::RbdReadContext {
            operation_features: lineage.operation_features,
            has_parent: lineage.has_parent,
            snapshot_id: lineage.snapshot_id,
            encrypted: lineage.encrypted,
        },
    }
}
