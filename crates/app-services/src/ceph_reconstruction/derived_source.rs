use std::path::{Path, PathBuf};

use domain::{
    CaseId, DataSource, DataSourceId, DataSourceKind, DataSourcePlatform, DataSourceProvenance,
    DataSourceProvenanceStatus,
};
use persistence_sqlite::repositories::{
    ceph_osd_repo::CephOsdRepo,
    ceph_rbd_lineage_repo::{
        CephRbdLineageAggregate, CephRbdLineageRecord, CephRbdLineageRepo, CephRbdReplicaRecord,
    },
    datasource_cluster_repo::DataSourceClusterRepo,
    datasource_repo::{DataSourceRepo, DataSourceStorage},
};
use thiserror::Error;

use crate::source_db;

use super::{discover_rbd_images_from_source_dbs, RadosReplicaSource, RbdImageDescriptor};

mod filesystem;
use filesystem::build_and_enumerate_source;

#[derive(Debug, Error)]
pub enum DerivedSourceError {
    #[error("Ceph cluster {0} was not found")]
    ClusterNotFound(String),
    #[error("Ceph cluster {cluster_id} is not ready: {state}")]
    ClusterNotReady { cluster_id: String, state: String },
    #[error("Ceph cluster has no complete OSD source set")]
    IncompleteCluster,
    #[error("Ceph source {data_source_id} has no usable OSD inventory")]
    MissingInventory { data_source_id: String },
    #[error("Ceph source {data_source_id} has conflicting OSD inventory")]
    ConflictingInventory { data_source_id: String },
    #[error("RBD reconstruction failed: {0}")]
    Reconstruction(String),
    #[error("RBD image {0} was not found")]
    ImageNotFound(String),
    #[error("RBD image {0} has no supported filesystem")]
    NoFilesystem(String),
    #[error("RBD derived source has an invalid {field}")]
    InvalidIdentity { field: &'static str },
    #[error("RBD logical volume layout is unsupported: {0}")]
    UnsupportedLvm(String),
    #[error("RBD derived source database failed: {0}")]
    Database(#[from] persistence_sqlite::DbError),
    #[error("RBD derived source I/O failed: {0}")]
    Io(#[from] std::io::Error),
}

pub type DerivedSourceResult<T> = Result<T, DerivedSourceError>;

#[derive(Debug, Clone)]
pub struct MaterializedRbdSource {
    pub data_source: DataSource,
    pub file_count: u64,
    pub directory_count: u64,
    pub total_size: u64,
}

pub fn materialize_rbd_sources_for_cluster(
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    case_id: &CaseId,
    cluster_id: &str,
) -> DerivedSourceResult<Vec<MaterializedRbdSource>> {
    let cluster = DataSourceClusterRepo::new(case_conn)
        .find_by_id(cluster_id)?
        .ok_or_else(|| DerivedSourceError::ClusterNotFound(cluster_id.to_string()))?;
    if cluster.import_state != "ready" {
        return Err(DerivedSourceError::ClusterNotReady {
            cluster_id: cluster_id.to_string(),
            state: cluster.import_state,
        });
    }
    if let Some(materialized) = load_ready_rbd_sources(case_conn, case_root, case_id, cluster_id)? {
        return Ok(materialized);
    }

    let parent_ids = DataSourceRepo::new(case_conn).find_ids_by_cluster(case_id, cluster_id)?;
    if parent_ids.len() != cluster.member_count as usize
        || parent_ids.len() != cluster.ready_count as usize
    {
        return Err(DerivedSourceError::IncompleteCluster);
    }
    if !cluster_has_osd_inventory(case_conn, case_root, case_id, &parent_ids)? {
        return Ok(Vec::new());
    }
    let (replicas, replica_records) =
        load_cluster_replicas(case_conn, case_root, case_id, &parent_ids)?;
    let descriptors = discover_rbd_images_from_source_dbs(&replicas)
        .map_err(|error| DerivedSourceError::Reconstruction(error.to_string()))?;

    let mut materialized = Vec::new();
    for descriptor in descriptors {
        materialized.push(materialize_one_rbd_source(
            case_conn,
            case_root,
            case_id,
            cluster_id,
            &replicas,
            &replica_records,
            descriptor,
        )?);
    }
    if materialized.is_empty() {
        return Err(DerivedSourceError::ImageNotFound(
            "no RBD image catalog entries".to_string(),
        ));
    }
    Ok(materialized)
}

fn load_ready_rbd_sources(
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    case_id: &CaseId,
    cluster_id: &str,
) -> DerivedSourceResult<Option<Vec<MaterializedRbdSource>>> {
    let mut materialized = Vec::new();
    for source in DataSourceRepo::new(case_conn)
        .find_by_case(case_id)?
        .into_iter()
        .filter(|source| source.kind == DataSourceKind::CephRbd)
    {
        let Some(lineage) = CephRbdLineageRepo::new(case_conn).find_by_data_source(&source.id.0)?
        else {
            continue;
        };
        if lineage.lineage.parent_cluster_id != cluster_id {
            continue;
        }
        let Some(storage) = DataSourceRepo::new(case_conn).find_storage(&source.id)? else {
            return Err(DerivedSourceError::Database(
                persistence_sqlite::DbError::System(format!(
                    "RBD derived source {} is missing storage metadata",
                    source.id.0
                )),
            ));
        };
        if storage.import_state != "ready" {
            return Ok(None);
        }
        let source_id = source.id.clone();
        materialized.push(source_summary(
            case_conn, case_root, case_id, &source_id, source,
        )?);
    }
    if materialized.is_empty() {
        Ok(None)
    } else {
        Ok(Some(materialized))
    }
}

fn cluster_has_osd_inventory(
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    case_id: &CaseId,
    parent_ids: &[DataSourceId],
) -> DerivedSourceResult<bool> {
    for source_id in parent_ids {
        let source =
            source_db::open_reconstruction_source_by_id(case_conn, case_root, case_id, source_id)
                .map_err(|error| {
                DerivedSourceError::Database(persistence_sqlite::DbError::System(error.to_string()))
            })?;
        if CephOsdRepo::new(&source.connection)
            .find_by_data_source(&source_id.0)?
            .iter()
            .any(|inventory| inventory.whoami.is_some())
        {
            return Ok(true);
        }
    }
    Ok(false)
}

fn load_cluster_replicas(
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    case_id: &CaseId,
    parent_ids: &[DataSourceId],
) -> DerivedSourceResult<(Vec<RadosReplicaSource>, Vec<CephRbdReplicaRecord>)> {
    let mut replicas = Vec::with_capacity(parent_ids.len());
    let mut records = Vec::with_capacity(parent_ids.len());
    for source_id in parent_ids {
        let source =
            source_db::open_reconstruction_source_by_id(case_conn, case_root, case_id, source_id)
                .map_err(|error| {
                DerivedSourceError::Database(persistence_sqlite::DbError::System(error.to_string()))
            })?;
        let source_db_path = source_db::registered_source_db_path(case_conn, case_root, source_id)?;
        let inventories = CephOsdRepo::new(&source.connection).find_by_data_source(&source_id.0)?;
        let candidates = inventories
            .into_iter()
            .filter(|inventory| inventory.whoami.is_some())
            .collect::<Vec<_>>();
        let inventory = match candidates.as_slice() {
            [inventory] => inventory,
            [] => continue,
            _ => {
                return Err(DerivedSourceError::ConflictingInventory {
                    data_source_id: source_id.0.clone(),
                })
            }
        };
        let osd_id = inventory
            .whoami
            .ok_or_else(|| DerivedSourceError::MissingInventory {
                data_source_id: source_id.0.clone(),
            })?;
        replicas.push(
            RadosReplicaSource::new(source_id.clone(), inventory.id.clone(), source_db_path)
                .map_err(|error| DerivedSourceError::Reconstruction(error.to_string()))?,
        );
        records.push(CephRbdReplicaRecord {
            ordinal: records.len() as u32,
            source_data_source_id: source_id.0.clone(),
            inventory_id: inventory.id.clone(),
            osd_id,
        });
    }
    Ok((replicas, records))
}

fn materialize_one_rbd_source(
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    case_id: &CaseId,
    cluster_id: &str,
    replicas: &[RadosReplicaSource],
    replica_records: &[CephRbdReplicaRecord],
    descriptor: RbdImageDescriptor,
) -> DerivedSourceResult<MaterializedRbdSource> {
    let data_source_id = derived_data_source_id(cluster_id, &descriptor.metadata.id)?;
    let data_source = DataSource {
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
    };

    if let Some(existing) = DataSourceRepo::new(case_conn)
        .find_by_case(case_id)?
        .into_iter()
        .find(|source| source.id == data_source_id)
    {
        let storage = DataSourceRepo::new(case_conn).find_storage(&data_source_id)?;
        if storage.is_some_and(|value| value.import_state == "ready") {
            return source_summary(case_conn, case_root, case_id, &data_source_id, existing);
        }
        crate::case_service::delete_data_source_in(case_conn, case_root, &data_source_id.0)
            .map_err(|error| {
                DerivedSourceError::Database(persistence_sqlite::DbError::System(format!(
                    "RBD derived source {} could not be reset for retry: {error}",
                    data_source_id.0
                )))
            })?;
    }

    let storage = DataSourceStorage::source_db(
        &data_source_id.0,
        Some(DataSourcePlatform::Linux.as_storage_str()),
        Some("vm_disk".to_string()),
    );
    let lineage = lineage_aggregate(&data_source_id, cluster_id, &descriptor, replica_records);
    persistence_sqlite::repositories::ceph_rbd_lineage_repo::validate_aggregate(&lineage)?;
    let transaction = case_conn
        .unchecked_transaction()
        .map_err(persistence_sqlite::DbError::from)?;
    DataSourceRepo::new(&transaction).insert_with_storage(case_id, &data_source, &storage)?;
    persistence_sqlite::repositories::ceph_rbd_lineage_repo::insert_aggregate_in_transaction(
        &transaction,
        &lineage,
    )?;
    transaction
        .commit()
        .map_err(persistence_sqlite::DbError::from)?;

    let result =
        build_and_enumerate_source(case_root, case_id, &data_source, replicas, &descriptor);
    match result {
        Ok(summary) => {
            DataSourceRepo::new(case_conn).update_import_state(&data_source_id, "ready", None)?;
            Ok(MaterializedRbdSource {
                data_source,
                ..summary
            })
        }
        Err(error) => {
            if let Err(state_error) = DataSourceRepo::new(case_conn).update_import_state(
                &data_source_id,
                "failed",
                Some(&error.to_string()),
            ) {
                tracing::warn!(
                    data_source_id = %data_source_id.0,
                    error = %state_error,
                    "Failed to persist the failed RBD derived-source state"
                );
            }
            Err(error)
        }
    }
}

fn source_summary(
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
    data_source: DataSource,
) -> DerivedSourceResult<MaterializedRbdSource> {
    let source_conn =
        source_db::open_ready_source_by_id(case_conn, case_root, case_id, data_source_id)
            .map_err(|error| {
                DerivedSourceError::Database(persistence_sqlite::DbError::System(error.to_string()))
            })?
            .connection;
    let file_count = persistence_sqlite::repositories::file_repo::FileRepo::new(&source_conn)
        .count_by_data_source(data_source_id)?;
    let directory_count = source_conn.query_row(
        "SELECT COUNT(*) FROM file_entries WHERE data_source_id = ?1 AND entry_type = 'directory'",
        [&data_source_id.0],
        |row| row.get::<_, u64>(0),
    ).map_err(persistence_sqlite::DbError::from)?;
    let total_size = source_conn.query_row(
        "SELECT COALESCE(SUM(size), 0) FROM file_entries WHERE data_source_id = ?1 AND entry_type = 'file'",
        [&data_source_id.0],
        |row| row.get::<_, u64>(0),
    ).map_err(persistence_sqlite::DbError::from)?;
    Ok(MaterializedRbdSource {
        data_source,
        file_count,
        directory_count,
        total_size,
    })
}

fn lineage_aggregate(
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
            expected_replica_count: replicas.len() as u32,
        },
        replicas: replicas.to_vec(),
    }
}

fn derived_data_source_id(cluster_id: &str, image_id: &str) -> DerivedSourceResult<DataSourceId> {
    validate_identity_component("cluster ID", cluster_id)?;
    validate_identity_component("image ID", image_id)?;
    Ok(DataSourceId(format!("rbd-{cluster_id}-{image_id}")))
}

fn validate_identity_component(field: &'static str, value: &str) -> DerivedSourceResult<()> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(DerivedSourceError::InvalidIdentity { field });
    }
    Ok(())
}

#[cfg(test)]
#[path = "../../tests/unit/ceph_reconstruction/derived_source.rs"]
mod tests;
