use std::{
    path::Path,
    sync::{atomic::AtomicBool, Arc},
};

use domain::{CaseId, CephScopeId, DataSourceId, DataSourceKind};
use persistence_sqlite::repositories::{
    ceph_osd_repo::CephOsdRepo,
    ceph_rbd_lineage_repo::{CephRbdLineageRepo, CephRbdReplicaRecord},
    datasource_repo::DataSourceRepo,
    linux_topology_artifact_repo::{LinuxTopologyArtifactRecord, LinuxTopologyArtifactRepo},
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

mod scope;
use scope::{has_osd_inventory, load_ceph_scope, reconstruction_parent_ids};

pub fn materialize_rbd_sources_for_scope(
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    case_id: &CaseId,
    ceph_scope_id: &CephScopeId,
) -> DerivedSourceResult<Vec<MaterializedRbdSource>> {
    materialize_rbd_sources_for_scope_with_cancel(
        case_conn,
        case_root,
        case_id,
        ceph_scope_id,
        Arc::new(AtomicBool::new(false)),
    )
}

pub fn materialize_rbd_sources_for_scope_with_cancel(
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    case_id: &CaseId,
    ceph_scope_id: &CephScopeId,
    cancel_token: Arc<AtomicBool>,
) -> DerivedSourceResult<Vec<MaterializedRbdSource>> {
    ensure_not_cancelled(&cancel_token)?;
    let scope = load_ceph_scope(case_conn, case_id, ceph_scope_id)?;
    if !matches!(scope.status.as_str(), "ready" | "partial") {
        return Err(DerivedSourceError::ScopeNotReady {
            scope_id: ceph_scope_id.0.clone(),
            state: scope.status,
        });
    }
    if let Some(materialized) =
        load_ready_rbd_sources(case_conn, case_root, case_id, ceph_scope_id)?
    {
        ensure_not_cancelled(&cancel_token)?;
        return Ok(materialized);
    }

    ensure_not_cancelled(&cancel_token)?;
    let parent_ids =
        DataSourceRepo::new(case_conn).find_ids_by_topology_scope(case_id, &ceph_scope_id.0)?;
    if parent_ids.len() != scope.member_count as usize {
        return Err(DerivedSourceError::IncompleteScope);
    }
    let reconstruction_parent_ids =
        reconstruction_parent_ids(case_conn, &parent_ids, &cancel_token)?;
    if !has_osd_inventory(
        case_conn,
        case_root,
        case_id,
        &reconstruction_parent_ids,
        &cancel_token,
    )? {
        write_empty_coverage_report(case_root, &ceph_scope_id.0)?;
        return Ok(Vec::new());
    }
    let (replicas, replica_records) = load_cluster_replicas(
        case_conn,
        case_root,
        case_id,
        &reconstruction_parent_ids,
        &cancel_token,
    )?;
    validate_map_inventory_membership(case_root, &ceph_scope_id.0, &replicas)?;
    let descriptors = discover_rbd_images_from_source_dbs_unbound(&replicas)
        .map_err(|error| DerivedSourceError::Reconstruction(error.to_string()))?;
    if descriptors.is_empty() {
        return Err(DerivedSourceError::ImageNotFound(
            "no RBD image catalog entries".to_string(),
        ));
    }
    let (policy, policy_diagnostics) =
        resolve_descriptor_policy(case_root, &ceph_scope_id.0, &descriptors, replicas.len())?;
    let coverage = assess_and_write_coverage(
        case_root,
        &ceph_scope_id.0,
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
        materialized.push(materialize_descriptor(
            case_conn,
            case_root,
            case_id,
            ceph_scope_id,
            &replicas,
            &replica_records,
            &policy,
            &cancel_token,
            descriptor,
        )?);
    }
    Ok(materialized)
}

#[allow(clippy::too_many_arguments)]
fn materialize_descriptor(
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    case_id: &CaseId,
    ceph_scope_id: &CephScopeId,
    replicas: &[RadosReplicaSource],
    replica_records: &[CephRbdReplicaRecord],
    policy: &RbdReplicaPolicy,
    cancel_token: &AtomicBool,
    descriptor: crate::ceph_reconstruction::RbdImageDescriptor,
) -> DerivedSourceResult<MaterializedRbdSource> {
    ensure_not_cancelled(cancel_token)?;
    let image_id = descriptor.metadata.id.clone();
    let source = materialize_one_rbd_source(
        RbdMaterializationContext {
            case_conn,
            case_root,
            case_id,
            ceph_scope_id: &ceph_scope_id.0,
            replicas,
            replica_records,
            policy,
            cancel_token,
        },
        descriptor,
    )?;
    LinuxTopologyArtifactRepo::new(case_conn).insert(&LinuxTopologyArtifactRecord {
        scope_id: ceph_scope_id.0.clone(),
        data_source_id: source.data_source.id.0.clone(),
        file_id: Some(format!("rbd:{image_id}")),
        layer: "storage".to_string(),
        artifact_kind: "ceph_rbd".to_string(),
        parser: "ceph-rbd-materialization".to_string(),
        status: "parsed".to_string(),
        diagnostics_json: "[]".to_string(),
        content_digest: Some(source.catalog_digest.clone()),
    })?;
    Ok(source)
}

fn validate_map_inventory_membership(
    case_root: &Path,
    ceph_scope_id: &str,
    replicas: &[RadosReplicaSource],
) -> DerivedSourceResult<()> {
    let identities = replicas
        .iter()
        .map(|replica| replica.identity.clone())
        .collect::<Vec<_>>();
    validate_inventory_membership(case_root, ceph_scope_id, &identities)
        .map_err(|error| DerivedSourceError::Reconstruction(error.to_string()))?;
    Ok(())
}

fn resolve_descriptor_policy(
    case_root: &Path,
    ceph_scope_id: &str,
    descriptors: &[crate::ceph_reconstruction::RbdImageDescriptor],
    observed_replica_count: usize,
) -> DerivedSourceResult<(RbdReplicaPolicy, Vec<String>)> {
    let mut selected_policy = None;
    let mut diagnostics = Vec::new();
    for descriptor in descriptors {
        let resolution = resolve_rbd_replica_policy(
            case_root,
            ceph_scope_id,
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

fn write_empty_coverage_report(case_root: &Path, ceph_scope_id: &str) -> DerivedSourceResult<()> {
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
    cluster_service::write_ceph_scope_coverage_report(case_root, ceph_scope_id, &report)
        .map_err(|error| DerivedSourceError::InconsistentState(error.to_string()))?;
    Ok(())
}

fn assess_and_write_coverage(
    case_root: &Path,
    ceph_scope_id: &str,
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
    cluster_service::write_ceph_scope_coverage_report(case_root, ceph_scope_id, &coverage)
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
    ceph_scope_id: &CephScopeId,
) -> DerivedSourceResult<Option<Vec<MaterializedRbdSource>>> {
    let mut materialized = Vec::new();
    if evidence_is_present(case_root, &ceph_scope_id.0)
        .map_err(|error| DerivedSourceError::Reconstruction(error.to_string()))?
    {
        let parent_ids =
            DataSourceRepo::new(case_conn).find_ids_by_topology_scope(case_id, &ceph_scope_id.0)?;
        let reconstruction_parent_ids =
            reconstruction_parent_ids(case_conn, &parent_ids, &AtomicBool::new(false))?;
        let (ready_replicas, _) = load_cluster_replicas(
            case_conn,
            case_root,
            case_id,
            &reconstruction_parent_ids,
            &AtomicBool::new(false),
        )?;
        validate_map_inventory_membership(case_root, &ceph_scope_id.0, &ready_replicas)?;
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
        if lineage.lineage.parent_ceph_scope_id != ceph_scope_id.0 {
            continue;
        }
        let current_policy = resolve_rbd_replica_policy(
            case_root,
            &ceph_scope_id.0,
            Some(lineage.lineage.data_pool_id),
            lineage.replicas.len(),
        )
        .map_err(|error| DerivedSourceError::Reconstruction(error.to_string()))?;
        let Some(coverage) =
            cluster_service::read_ceph_scope_coverage_report(case_root, &ceph_scope_id.0)
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
