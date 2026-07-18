use std::path::{Path, PathBuf};

use domain::{
    CaseId, DataSource, DataSourceId, DataSourceKind, DataSourcePlatform, DataSourceProvenance,
    DataSourceProvenanceStatus,
};
use persistence_sqlite::repositories::{
    ceph_rbd_lineage_repo::{CephRbdLineageAggregate, CephRbdLineageRecord, CephRbdReplicaRecord},
    datasource_repo::{DataSourceRepo, DataSourceStorage},
};

use super::{
    catalog_manifest::{load_current_source_summary, persist_current_source_manifest},
    derived_data_source_id,
    filesystem::build_and_enumerate_source,
    DerivedSourceError, DerivedSourceResult, MaterializedRbdSource,
};
use crate::ceph_reconstruction::{
    derived_finalizer::{
        begin_catalog_phase, catalog_phase_is_current, complete_catalog_phase, fail_catalog_phase,
        finalize_derived_source, start_catalog_heartbeat, DerivedFinalizationReport, PhaseClaim,
        ProcessingPhaseAttempt,
    },
    load_lineage_fingerprint, RadosReplicaSource, RbdImageDescriptor,
};

pub(super) fn materialize_one_rbd_source(
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    case_id: &CaseId,
    cluster_id: &str,
    replicas: &[RadosReplicaSource],
    replica_records: &[CephRbdReplicaRecord],
    descriptor: RbdImageDescriptor,
) -> DerivedSourceResult<MaterializedRbdSource> {
    let data_source_id = derived_data_source_id(cluster_id, &descriptor.metadata.id)?;
    if let Some(ready) = reuse_or_reset_existing(case_conn, case_root, case_id, &data_source_id)? {
        return Ok(ready);
    }
    let data_source = build_data_source(cluster_id, &data_source_id, &descriptor);
    let fingerprint = register_derived_source(
        case_conn,
        case_id,
        cluster_id,
        &data_source,
        &descriptor,
        replica_records,
    )?;
    let catalog_attempt = start_catalog(case_conn, &data_source_id, &fingerprint)?;
    let catalog_heartbeat =
        match start_catalog_heartbeat(case_conn, &data_source_id, &fingerprint, &catalog_attempt) {
            Ok(heartbeat) => heartbeat,
            Err(error) => {
                let error = DerivedSourceError::Database(error);
                record_catalog_failure(
                    case_conn,
                    &data_source_id,
                    &fingerprint,
                    &catalog_attempt,
                    &error,
                );
                return Err(error);
            }
        };
    let catalog_result =
        build_and_enumerate_source(case_root, case_id, &data_source, replicas, &descriptor);
    drop(catalog_heartbeat);
    match catalog_result {
        Ok(summary) => match finish_materialization(
            case_conn,
            case_root,
            case_id,
            data_source,
            summary,
            &fingerprint,
            &catalog_attempt,
        ) {
            Ok(materialized) => Ok(materialized),
            Err(error) => {
                record_catalog_failure(
                    case_conn,
                    &data_source_id,
                    &fingerprint,
                    &catalog_attempt,
                    &error,
                );
                Err(error)
            }
        },
        Err(error) => {
            record_catalog_failure(
                case_conn,
                &data_source_id,
                &fingerprint,
                &catalog_attempt,
                &error,
            );
            Err(error)
        }
    }
}

fn reuse_or_reset_existing(
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
) -> DerivedSourceResult<Option<MaterializedRbdSource>> {
    let Some(existing) = DataSourceRepo::new(case_conn)
        .find_by_case(case_id)?
        .into_iter()
        .find(|source| source.id == *data_source_id)
    else {
        return Ok(None);
    };
    let storage = DataSourceRepo::new(case_conn).find_storage(data_source_id)?;
    if storage.is_some_and(|value| value.import_state == "ready") {
        if let Some(summary) = ready_source_summary_if_current(case_conn, case_root, existing)? {
            finalize_ready_source(case_conn, case_root, case_id, data_source_id)?;
            return Ok(Some(summary));
        }
    }
    crate::case_service::delete_data_source_in(case_conn, case_root, &data_source_id.0).map_err(
        |error| {
            DerivedSourceError::Database(persistence_sqlite::DbError::System(format!(
                "RBD derived source {} could not be reset for retry: {error}",
                data_source_id.0
            )))
        },
    )?;
    Ok(None)
}

fn build_data_source(
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

fn register_derived_source(
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

fn start_catalog(
    case_conn: &rusqlite::Connection,
    data_source_id: &DataSourceId,
    fingerprint: &str,
) -> DerivedSourceResult<ProcessingPhaseAttempt> {
    match begin_catalog_phase(case_conn, data_source_id, fingerprint) {
        Ok(PhaseClaim::Acquired(attempt)) => Ok(attempt),
        Ok(PhaseClaim::Ready(_)) => Err(DerivedSourceError::InconsistentState(
            "catalog phase is ready while the derived source is not ready".to_string(),
        )),
        Ok(PhaseClaim::Busy(_)) => Err(DerivedSourceError::ProcessingBusy { phase: "catalog" }),
        Err(error) => Err(DerivedSourceError::Database(error)),
    }
}

fn finish_materialization(
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    case_id: &CaseId,
    data_source: DataSource,
    summary: MaterializedRbdSource,
    fingerprint: &str,
    catalog_attempt: &ProcessingPhaseAttempt,
) -> DerivedSourceResult<MaterializedRbdSource> {
    let source_connection = crate::source_db::open_source_db(case_root, &data_source.id)?;
    persist_current_source_manifest(&source_connection, fingerprint, &summary)?;
    crate::source_db::checkpoint_source_db(&source_connection)?;
    complete_catalog_phase(
        case_conn,
        &data_source.id,
        fingerprint,
        catalog_attempt,
        &summary,
    )?;
    DataSourceRepo::new(case_conn).update_import_state(&data_source.id, "ready", None)?;
    let report =
        finalize_derived_source(case_conn, case_root, case_id, &data_source.id, fingerprint);
    log_finalization_report(&data_source.id, &report);
    Ok(MaterializedRbdSource {
        data_source,
        ..summary
    })
}

fn record_catalog_failure(
    case_conn: &rusqlite::Connection,
    data_source_id: &DataSourceId,
    fingerprint: &str,
    attempt: &ProcessingPhaseAttempt,
    error: &DerivedSourceError,
) {
    fail_catalog_phase(
        case_conn,
        data_source_id,
        fingerprint,
        attempt,
        &error.to_string(),
    );
    if let Err(state_error) = DataSourceRepo::new(case_conn).update_import_state(
        data_source_id,
        "failed",
        Some(&error.to_string()),
    ) {
        tracing::warn!(
            data_source_id = %data_source_id.0,
            error = %state_error,
            "Failed to persist the failed RBD derived-source state"
        );
    }
}

pub(super) fn finalize_ready_source(
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
) -> DerivedSourceResult<()> {
    let fingerprint = load_lineage_fingerprint(case_conn, data_source_id)
        .map_err(|error| DerivedSourceError::Reconstruction(error.to_string()))?;
    if !catalog_phase_is_current(case_conn, data_source_id, &fingerprint)? {
        return Err(DerivedSourceError::InconsistentState(
            "ready derived source has a stale Catalog phase".to_string(),
        ));
    }
    let report =
        finalize_derived_source(case_conn, case_root, case_id, data_source_id, &fingerprint);
    log_finalization_report(data_source_id, &report);
    Ok(())
}

pub(super) fn ready_source_summary_if_current(
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    data_source: DataSource,
) -> DerivedSourceResult<Option<MaterializedRbdSource>> {
    let data_source_id = data_source.id.clone();
    let lineage_fingerprint = match load_lineage_fingerprint(case_conn, &data_source_id) {
        Ok(fingerprint) => fingerprint,
        Err(error) => {
            tracing::warn!(
                data_source_id = %data_source_id.0,
                error = %error,
                "Ready RBD derived source has no valid lineage fingerprint"
            );
            return Ok(None);
        }
    };
    if !catalog_phase_is_current(case_conn, &data_source_id, &lineage_fingerprint)? {
        return Ok(None);
    }
    let source_connection = crate::source_db::open_registered_source_db_read_only(
        case_conn,
        case_root,
        &data_source_id,
    )?;
    load_current_source_summary(&source_connection, &lineage_fingerprint, data_source)
}

fn log_finalization_report(data_source_id: &DataSourceId, report: &DerivedFinalizationReport) {
    let failed = report.failed_count();
    let deferred = report.deferred_count();
    if failed > 0 {
        tracing::warn!(
            data_source_id = %data_source_id.0,
            failed_phases = failed,
            deferred_phases = deferred,
            "RBD derived source is ready, but post-catalog processing is incomplete"
        );
    } else {
        tracing::info!(
            data_source_id = %data_source_id.0,
            deferred_phases = deferred,
            "RBD derived source post-catalog processing completed"
        );
    }
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
