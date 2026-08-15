use std::{
    path::Path,
    sync::{atomic::AtomicBool, Arc},
};

use domain::{CaseId, DataSourceId, DataSourceKind};
use persistence_sqlite::repositories::{
    ceph_osd_repo::CephOsdRepo,
    ceph_rbd_lineage_repo::{CephRbdLineageRepo, CephRbdReplicaRecord},
    datasource_cluster_repo::DataSourceClusterRepo,
    datasource_repo::DataSourceRepo,
};

use crate::{
    ceph_reconstruction::{discover_rbd_images_from_source_dbs, RadosReplicaSource},
    source_db,
};

use super::materialization::{
    finalize_ready_source, materialize_one_rbd_source, RbdMaterializationContext,
};
use super::{
    catalog_manifest, ensure_not_cancelled, materialization, DerivedSourceError,
    DerivedSourceResult, MaterializedRbdSource,
};

pub fn materialize_rbd_sources_for_cluster(
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    case_id: &CaseId,
    cluster_id: &str,
) -> DerivedSourceResult<Vec<MaterializedRbdSource>> {
    materialize_rbd_sources_for_cluster_with_cancel(
        case_conn,
        case_root,
        case_id,
        cluster_id,
        Arc::new(AtomicBool::new(false)),
    )
}

pub fn materialize_rbd_sources_for_cluster_with_cancel(
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    case_id: &CaseId,
    cluster_id: &str,
    cancel_token: Arc<AtomicBool>,
) -> DerivedSourceResult<Vec<MaterializedRbdSource>> {
    ensure_not_cancelled(&cancel_token)?;
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
        ensure_not_cancelled(&cancel_token)?;
        return Ok(materialized);
    }

    ensure_not_cancelled(&cancel_token)?;
    let parent_ids = DataSourceRepo::new(case_conn).find_ids_by_cluster(case_id, cluster_id)?;
    if parent_ids.len() != cluster.member_count as usize
        || parent_ids.len() != cluster.ready_count as usize
    {
        return Err(DerivedSourceError::IncompleteCluster);
    }
    let reconstruction_parent_ids =
        reconstruction_parent_ids(case_conn, &parent_ids, &cancel_token)?;
    if !cluster_has_osd_inventory(
        case_conn,
        case_root,
        case_id,
        &reconstruction_parent_ids,
        &cancel_token,
    )? {
        return Ok(Vec::new());
    }
    let (replicas, replica_records) = load_cluster_replicas(
        case_conn,
        case_root,
        case_id,
        &reconstruction_parent_ids,
        &cancel_token,
    )?;
    ensure_not_cancelled(&cancel_token)?;
    let descriptors = discover_rbd_images_from_source_dbs(&replicas)
        .map_err(|error| DerivedSourceError::Reconstruction(error.to_string()))?;
    ensure_not_cancelled(&cancel_token)?;

    let mut materialized = Vec::new();
    for descriptor in descriptors {
        ensure_not_cancelled(&cancel_token)?;
        materialized.push(materialize_one_rbd_source(
            RbdMaterializationContext {
                case_conn,
                case_root,
                case_id,
                cluster_id,
                replicas: &replicas,
                replica_records: &replica_records,
                cancel_token: &cancel_token,
            },
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

pub fn finalize_rbd_source_processing(
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
) -> DerivedSourceResult<()> {
    finalize_rbd_source_processing_with_cancel(
        case_conn,
        case_root,
        case_id,
        data_source_id,
        Arc::new(AtomicBool::new(false)),
    )
}

pub fn finalize_rbd_source_processing_with_cancel(
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
    cancel_token: Arc<AtomicBool>,
) -> DerivedSourceResult<()> {
    let belongs_to_case = DataSourceRepo::new(case_conn)
        .find_by_case(case_id)?
        .into_iter()
        .any(|source| source.id == *data_source_id && source.kind == DataSourceKind::CephRbd);
    if !belongs_to_case {
        return Err(DerivedSourceError::InconsistentState(format!(
            "derived source {} does not belong to case {}",
            data_source_id.0, case_id.0
        )));
    }
    finalize_ready_source(case_conn, case_root, case_id, data_source_id, cancel_token)
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
        let Some(summary) =
            materialization::ready_source_summary_if_current(case_conn, case_root, source)?
        else {
            return Ok(None);
        };
        materialized.push(summary);
    }
    if materialized.is_empty() {
        Ok(None)
    } else {
        Ok(Some(materialized))
    }
}

fn reconstruction_parent_ids(
    case_conn: &rusqlite::Connection,
    parent_ids: &[DataSourceId],
    cancel_token: &AtomicBool,
) -> DerivedSourceResult<Vec<DataSourceId>> {
    let repo = DataSourceRepo::new(case_conn);
    let mut reconstruction_sources = Vec::new();
    for data_source_id in parent_ids {
        ensure_not_cancelled(cancel_token)?;
        let storage = repo.find_storage(data_source_id)?.ok_or_else(|| {
            DerivedSourceError::InconsistentState(format!(
                "cluster member {} is missing storage metadata",
                data_source_id.0
            ))
        })?;
        if storage.import_state == "ready_metadata" {
            reconstruction_sources.push(data_source_id.clone());
        }
    }
    Ok(reconstruction_sources)
}

fn cluster_has_osd_inventory(
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    case_id: &CaseId,
    parent_ids: &[DataSourceId],
    cancel_token: &AtomicBool,
) -> DerivedSourceResult<bool> {
    for source_id in parent_ids {
        ensure_not_cancelled(cancel_token)?;
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
    cancel_token: &AtomicBool,
) -> DerivedSourceResult<(Vec<RadosReplicaSource>, Vec<CephRbdReplicaRecord>)> {
    let mut replicas = Vec::with_capacity(parent_ids.len());
    let mut records = Vec::with_capacity(parent_ids.len());
    for source_id in parent_ids {
        ensure_not_cancelled(cancel_token)?;
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

pub fn verify_derived_source_catalog(
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
) -> DerivedSourceResult<bool> {
    let source = DataSourceRepo::new(case_conn)
        .find_by_case(case_id)?
        .into_iter()
        .find(|source| source.id == *data_source_id && source.kind == DataSourceKind::CephRbd)
        .ok_or_else(|| {
            DerivedSourceError::InconsistentState(format!(
                "derived source {} does not belong to case {}",
                data_source_id.0, case_id.0
            ))
        })?;
    let lineage_fingerprint =
        crate::ceph_reconstruction::load_lineage_fingerprint(case_conn, data_source_id)
            .map_err(|error| DerivedSourceError::Reconstruction(error.to_string()))?;
    let connection =
        source_db::open_registered_source_db_read_only(case_conn, case_root, data_source_id)?;
    catalog_manifest::verify_current_source_manifest_deep(&connection, &lineage_fingerprint, source)
}
