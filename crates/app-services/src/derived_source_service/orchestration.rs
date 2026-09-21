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
    ceph_reconstruction::{
        assess_inventory_coverage, discover_rbd_images_from_source_dbs_unbound,
        evidence_is_present, resolve_rbd_replica_policy, validate_inventory_membership,
        InventoryEvidence, RadosReplicaSource, RbdReplicaPolicy, ReplicaIdentity,
    },
    cluster_service, source_db,
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
    crate::cluster_service::LinuxClusterKind::from_profile(cluster.profile.as_deref())
        .require_pve_ceph()
        .map_err(|_| DerivedSourceError::IncompleteCluster)?;
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
        write_empty_coverage_report(case_root, cluster_id)?;
        return Ok(Vec::new());
    }
    let (replicas, replica_records) = load_cluster_replicas(
        case_conn,
        case_root,
        case_id,
        &reconstruction_parent_ids,
        &cancel_token,
    )?;
    validate_map_inventory_membership(case_root, cluster_id, &replicas)?;
    let descriptors = discover_rbd_images_from_source_dbs_unbound(&replicas)
        .map_err(|error| DerivedSourceError::Reconstruction(error.to_string()))?;
    if descriptors.is_empty() {
        return Err(DerivedSourceError::ImageNotFound(
            "no RBD image catalog entries".to_string(),
        ));
    }
    let (policy, policy_diagnostics) =
        resolve_descriptor_policy(case_root, cluster_id, &descriptors, replicas.len())?;
    let coverage = assess_and_write_coverage(
        case_root,
        cluster_id,
        &replicas,
        &policy,
        &policy_diagnostics,
    )?;
    ensure_complete_coverage(&coverage)?;
    ensure_not_cancelled(&cancel_token)?;
    for descriptor in &descriptors {
        policy
            .validate_for_pool(descriptor.metadata.data_pool_id)
            .map_err(|error| DerivedSourceError::Reconstruction(error.to_string()))?;
    }
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
                policy: &policy,
                cancel_token: &cancel_token,
            },
            descriptor,
        )?);
    }
    Ok(materialized)
}

fn validate_map_inventory_membership(
    case_root: &Path,
    cluster_id: &str,
    replicas: &[RadosReplicaSource],
) -> DerivedSourceResult<()> {
    let identities = replicas
        .iter()
        .map(|replica| replica.identity.clone())
        .collect::<Vec<_>>();
    validate_inventory_membership(case_root, cluster_id, &identities)
        .map_err(|error| DerivedSourceError::Reconstruction(error.to_string()))?;
    Ok(())
}

fn resolve_descriptor_policy(
    case_root: &Path,
    cluster_id: &str,
    descriptors: &[crate::ceph_reconstruction::RbdImageDescriptor],
    observed_replica_count: usize,
) -> DerivedSourceResult<(RbdReplicaPolicy, Vec<String>)> {
    let mut selected_policy = None;
    let mut diagnostics = Vec::new();
    for descriptor in descriptors {
        let resolution = resolve_rbd_replica_policy(
            case_root,
            cluster_id,
            Some(descriptor.metadata.data_pool_id),
            observed_replica_count,
        )
        .map_err(|error| DerivedSourceError::Reconstruction(error.to_string()))?;
        if let Some(policy) = &selected_policy {
            if policy != &resolution.policy {
                return Err(DerivedSourceError::InconsistentState(
                    "RBD descriptors require different replica policies".to_string(),
                ));
            }
        } else {
            selected_policy = Some(resolution.policy.clone());
        }
        for diagnostic in resolution.diagnostics {
            if !diagnostics.contains(&diagnostic) {
                diagnostics.push(diagnostic);
            }
        }
    }
    selected_policy
        .map(|policy| (policy, diagnostics))
        .ok_or_else(|| {
            DerivedSourceError::ImageNotFound("no RBD image catalog entries".to_string())
        })
}

fn ensure_complete_coverage(
    coverage: &crate::ceph_reconstruction::InventoryCoverageReport,
) -> DerivedSourceResult<()> {
    if coverage.is_complete() {
        return Ok(());
    }
    tracing::warn!(
        state = coverage.state.as_str(),
        diagnostics = ?coverage.diagnostics,
        "RBD inventory coverage is not fully proven; refusing materialization"
    );
    Err(DerivedSourceError::ReplicaCoverageNotProven {
        state: coverage.state.as_str().to_string(),
    })
}

fn write_empty_coverage_report(case_root: &Path, cluster_id: &str) -> DerivedSourceResult<()> {
    let report = crate::ceph_reconstruction::InventoryCoverageReport {
        policy: RbdReplicaPolicy::strict_legacy(),
        expected_count: RbdReplicaPolicy::strict_legacy().expected_count(),
        observed_count: 0,
        state: crate::ceph_reconstruction::InventoryCoverageState::Incomplete,
        duplicate_inventory_ids: Vec::new(),
        duplicate_source_ids: Vec::new(),
        duplicate_osd_ids: Vec::new(),
        ceph_fsids: Vec::new(),
        diagnostics: vec!["no usable OSD inventory was found".to_string()],
    };
    cluster_service::write_linux_cluster_coverage_report(case_root, cluster_id, &report)
        .map_err(|error| DerivedSourceError::InconsistentState(error.to_string()))?;
    Ok(())
}

fn assess_and_write_coverage(
    case_root: &Path,
    cluster_id: &str,
    replicas: &[RadosReplicaSource],
    policy: &RbdReplicaPolicy,
    policy_diagnostics: &[String],
) -> DerivedSourceResult<crate::ceph_reconstruction::InventoryCoverageReport> {
    let evidence = replicas
        .iter()
        .map(|replica| InventoryEvidence {
            source_id: replica.data_source_id.0.clone(),
            inventory_id: replica.inventory_id.clone(),
            identity: replica.identity.clone(),
        })
        .collect::<Vec<_>>();
    let mut coverage = assess_inventory_coverage(&evidence, policy);
    for diagnostic in policy_diagnostics {
        if !coverage.diagnostics.contains(diagnostic) {
            coverage.diagnostics.push(diagnostic.clone());
        }
    }
    cluster_service::write_linux_cluster_coverage_report(case_root, cluster_id, &coverage)
        .map_err(|error| DerivedSourceError::InconsistentState(error.to_string()))?;
    Ok(coverage)
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
    if evidence_is_present(case_root, cluster_id)
        .map_err(|error| DerivedSourceError::Reconstruction(error.to_string()))?
    {
        let parent_ids = DataSourceRepo::new(case_conn).find_ids_by_cluster(case_id, cluster_id)?;
        let reconstruction_parent_ids =
            reconstruction_parent_ids(case_conn, &parent_ids, &AtomicBool::new(false))?;
        let (ready_replicas, _) = load_cluster_replicas(
            case_conn,
            case_root,
            case_id,
            &reconstruction_parent_ids,
            &AtomicBool::new(false),
        )?;
        validate_map_inventory_membership(case_root, cluster_id, &ready_replicas)?;
    }
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
        let current_policy = resolve_rbd_replica_policy(
            case_root,
            cluster_id,
            Some(lineage.lineage.data_pool_id),
            lineage.replicas.len(),
        )
        .map_err(|error| DerivedSourceError::Reconstruction(error.to_string()))?;
        let Some(coverage) =
            cluster_service::read_linux_cluster_coverage_report(case_root, cluster_id)
                .map_err(|error| DerivedSourceError::InconsistentState(error.to_string()))?
        else {
            return Ok(None);
        };
        if !coverage.is_complete() {
            return Ok(None);
        }
        if coverage.policy != current_policy.policy {
            return Ok(None);
        }
        if lineage.lineage.replica_policy_fingerprint.is_empty()
            || lineage.lineage.replica_policy_fingerprint != current_policy.policy.fingerprint()
        {
            return Ok(None);
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
            RadosReplicaSource::with_identity(
                source_id.clone(),
                inventory.id.clone(),
                source_db_path,
                ReplicaIdentity::from_inventory(
                    inventory.whoami,
                    inventory.osd_uuid.clone(),
                    inventory.ceph_fsid.clone(),
                ),
            )
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
