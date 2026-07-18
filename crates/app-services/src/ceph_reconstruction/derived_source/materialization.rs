use std::{
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use domain::{CaseId, DataSource, DataSourceId};
use persistence_sqlite::repositories::{
    ceph_rbd_lineage_repo::CephRbdReplicaRecord, datasource_repo::DataSourceRepo,
};

use super::{
    catalog_build::{build_and_enumerate_source, CatalogBuildRequest},
    catalog_manifest::load_current_source_summary,
    derived_data_source_id, DerivedSourceError, DerivedSourceResult, MaterializedRbdSource,
};
use crate::ceph_reconstruction::{
    derived_finalizer::{
        begin_catalog_phase, catalog_phase_is_current, complete_catalog_phase, defer_catalog_phase,
        fail_catalog_phase, finalize_derived_source, queue_post_catalog_phases,
        start_catalog_heartbeat, DerivedFinalizationReport, PhaseClaim, ProcessingPhaseAttempt,
    },
    load_lineage_fingerprint, RadosReplicaSource, RbdImageDescriptor,
};

mod recovery;
mod registration;

use recovery::reuse_existing_catalog;
use registration::{build_data_source, register_derived_source, validate_existing_registration};

pub(super) struct RbdMaterializationContext<'a> {
    pub(super) case_conn: &'a rusqlite::Connection,
    pub(super) case_root: &'a Path,
    pub(super) case_id: &'a CaseId,
    pub(super) cluster_id: &'a str,
    pub(super) replicas: &'a [RadosReplicaSource],
    pub(super) replica_records: &'a [CephRbdReplicaRecord],
    pub(super) cancel_token: &'a AtomicBool,
}

enum PreparedDerivedSource {
    Ready(MaterializedRbdSource),
    Pending {
        data_source: DataSource,
        fingerprint: String,
    },
}

pub(super) fn materialize_one_rbd_source(
    context: RbdMaterializationContext<'_>,
    descriptor: RbdImageDescriptor,
) -> DerivedSourceResult<MaterializedRbdSource> {
    ensure_not_cancelled(context.cancel_token)?;
    let data_source_id = derived_data_source_id(context.cluster_id, &descriptor.metadata.id)?;
    match prepare_derived_source(&context, &data_source_id, &descriptor)? {
        PreparedDerivedSource::Ready(ready) => Ok(ready),
        PreparedDerivedSource::Pending {
            data_source,
            fingerprint,
        } => run_catalog_materialization(
            context,
            descriptor,
            data_source_id,
            data_source,
            fingerprint,
        ),
    }
}

fn prepare_derived_source(
    context: &RbdMaterializationContext<'_>,
    data_source_id: &DataSourceId,
    descriptor: &RbdImageDescriptor,
) -> DerivedSourceResult<PreparedDerivedSource> {
    let desired_source = build_data_source(context.cluster_id, data_source_id, descriptor);
    let existing_source = DataSourceRepo::new(context.case_conn)
        .find_by_case(context.case_id)?
        .into_iter()
        .find(|source| source.id == *data_source_id);
    let (data_source, fingerprint) = match existing_source {
        Some(existing_source) => {
            if let Some(ready) = reuse_existing_catalog(
                context.case_conn,
                context.case_root,
                context.case_id,
                data_source_id,
            )? {
                ensure_not_cancelled(context.cancel_token)?;
                return Ok(PreparedDerivedSource::Ready(ready));
            }
            let fingerprint = validate_existing_registration(
                context.case_conn,
                context.cluster_id,
                &existing_source,
                &desired_source,
                descriptor,
                context.replica_records,
            )?;
            (existing_source, fingerprint)
        }
        None => {
            ensure_not_cancelled(context.cancel_token)?;
            let fingerprint = register_derived_source(
                context.case_conn,
                context.case_id,
                context.cluster_id,
                &desired_source,
                descriptor,
                context.replica_records,
            )?;
            (desired_source, fingerprint)
        }
    };
    Ok(PreparedDerivedSource::Pending {
        data_source,
        fingerprint,
    })
}

fn run_catalog_materialization(
    context: RbdMaterializationContext<'_>,
    descriptor: RbdImageDescriptor,
    data_source_id: DataSourceId,
    data_source: DataSource,
    fingerprint: String,
) -> DerivedSourceResult<MaterializedRbdSource> {
    let catalog_attempt = start_catalog(context.case_conn, &data_source_id, &fingerprint)?;
    let catalog_heartbeat = match start_catalog_heartbeat(
        context.case_conn,
        &data_source_id,
        &fingerprint,
        &catalog_attempt,
    ) {
        Ok(heartbeat) => heartbeat,
        Err(error) => {
            let error = DerivedSourceError::Database(error);
            record_catalog_failure(
                context.case_conn,
                &data_source_id,
                &fingerprint,
                &catalog_attempt,
                &error,
            );
            return Err(error);
        }
    };
    let catalog_result = build_and_enumerate_source(CatalogBuildRequest {
        case_conn: context.case_conn,
        case_root: context.case_root,
        case_id: context.case_id,
        data_source: &data_source,
        replicas: context.replicas,
        descriptor: &descriptor,
        lineage_fingerprint: &fingerprint,
        catalog_attempt: &catalog_attempt,
        cancel_token: context.cancel_token,
    });
    let lease_lost = catalog_heartbeat.lease_lost();
    drop(catalog_heartbeat);
    if lease_lost && catalog_result.is_ok() {
        let error = DerivedSourceError::ProcessingBusy { phase: "catalog" };
        record_catalog_failure(
            context.case_conn,
            &data_source_id,
            &fingerprint,
            &catalog_attempt,
            &error,
        );
        return Err(error);
    }
    handle_catalog_result(
        context.case_conn,
        data_source,
        &data_source_id,
        &fingerprint,
        &catalog_attempt,
        context.cancel_token,
        catalog_result,
    )
}

fn handle_catalog_result(
    case_conn: &rusqlite::Connection,
    data_source: DataSource,
    data_source_id: &DataSourceId,
    fingerprint: &str,
    catalog_attempt: &ProcessingPhaseAttempt,
    cancel_token: &AtomicBool,
    catalog_result: DerivedSourceResult<MaterializedRbdSource>,
) -> DerivedSourceResult<MaterializedRbdSource> {
    match catalog_result {
        Ok(summary) => match finish_materialization(
            case_conn,
            data_source,
            summary,
            fingerprint,
            catalog_attempt,
            cancel_token,
        ) {
            Ok(materialized) => Ok(materialized),
            Err(DerivedSourceError::ProcessingCancelled) => {
                record_catalog_deferred(case_conn, data_source_id, fingerprint, catalog_attempt);
                Err(DerivedSourceError::ProcessingCancelled)
            }
            Err(error) => {
                record_catalog_failure(
                    case_conn,
                    data_source_id,
                    fingerprint,
                    catalog_attempt,
                    &error,
                );
                Err(error)
            }
        },
        Err(DerivedSourceError::ProcessingCancelled) => {
            record_catalog_deferred(case_conn, data_source_id, fingerprint, catalog_attempt);
            Err(DerivedSourceError::ProcessingCancelled)
        }
        Err(error) => {
            record_catalog_failure(
                case_conn,
                data_source_id,
                fingerprint,
                catalog_attempt,
                &error,
            );
            Err(error)
        }
    }
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
    data_source: DataSource,
    summary: MaterializedRbdSource,
    fingerprint: &str,
    catalog_attempt: &ProcessingPhaseAttempt,
    cancel_token: &AtomicBool,
) -> DerivedSourceResult<MaterializedRbdSource> {
    ensure_not_cancelled(cancel_token)?;
    publish_catalog_readiness(
        case_conn,
        &data_source,
        fingerprint,
        catalog_attempt,
        &summary,
    )?;
    Ok(MaterializedRbdSource {
        data_source,
        ..summary
    })
}

fn publish_catalog_readiness(
    case_conn: &rusqlite::Connection,
    data_source: &DataSource,
    fingerprint: &str,
    catalog_attempt: &ProcessingPhaseAttempt,
    summary: &MaterializedRbdSource,
) -> DerivedSourceResult<()> {
    let transaction = case_conn
        .unchecked_transaction()
        .map_err(persistence_sqlite::DbError::from)?;
    complete_catalog_phase(
        &transaction,
        &data_source.id,
        fingerprint,
        catalog_attempt,
        summary,
    )?;
    queue_post_catalog_phases(&transaction, &data_source.id, fingerprint)?;
    DataSourceRepo::new(&transaction).update_import_state(&data_source.id, "ready", None)?;
    transaction
        .commit()
        .map_err(persistence_sqlite::DbError::from)?;
    Ok(())
}

fn record_catalog_deferred(
    case_conn: &rusqlite::Connection,
    data_source_id: &DataSourceId,
    fingerprint: &str,
    attempt: &ProcessingPhaseAttempt,
) {
    const REASON: &str = "RBD Catalog materialization cancelled; retry is safe";
    if let Err(state_error) =
        persist_catalog_deferred(case_conn, data_source_id, fingerprint, attempt, REASON)
    {
        tracing::warn!(
            data_source_id = %data_source_id.0,
            error = %state_error,
            "Failed to persist the deferred RBD derived-source state"
        );
    }
}

fn persist_catalog_deferred(
    case_conn: &rusqlite::Connection,
    data_source_id: &DataSourceId,
    fingerprint: &str,
    attempt: &ProcessingPhaseAttempt,
    reason: &str,
) -> DerivedSourceResult<()> {
    let transaction = case_conn
        .unchecked_transaction()
        .map_err(persistence_sqlite::DbError::from)?;
    defer_catalog_phase(&transaction, data_source_id, fingerprint, attempt, reason)?;
    DataSourceRepo::new(&transaction).update_import_state(
        data_source_id,
        "pending",
        Some(reason),
    )?;
    transaction
        .commit()
        .map_err(persistence_sqlite::DbError::from)?;
    Ok(())
}

fn record_catalog_failure(
    case_conn: &rusqlite::Connection,
    data_source_id: &DataSourceId,
    fingerprint: &str,
    attempt: &ProcessingPhaseAttempt,
    error: &DerivedSourceError,
) {
    if let Err(state_error) =
        persist_catalog_failure(case_conn, data_source_id, fingerprint, attempt, error)
    {
        tracing::warn!(
            data_source_id = %data_source_id.0,
            error = %state_error,
            "Failed to persist the failed RBD derived-source state"
        );
    }
}

fn persist_catalog_failure(
    case_conn: &rusqlite::Connection,
    data_source_id: &DataSourceId,
    fingerprint: &str,
    attempt: &ProcessingPhaseAttempt,
    error: &DerivedSourceError,
) -> DerivedSourceResult<()> {
    let transaction = case_conn
        .unchecked_transaction()
        .map_err(persistence_sqlite::DbError::from)?;
    fail_catalog_phase(
        &transaction,
        data_source_id,
        fingerprint,
        attempt,
        &error.to_string(),
    )?;
    DataSourceRepo::new(&transaction).update_import_state(
        data_source_id,
        "failed",
        Some(&error.to_string()),
    )?;
    transaction
        .commit()
        .map_err(persistence_sqlite::DbError::from)?;
    Ok(())
}

pub(super) fn finalize_ready_source(
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
    cancel_token: Arc<AtomicBool>,
) -> DerivedSourceResult<()> {
    if cancel_token.load(Ordering::Relaxed) {
        return Err(DerivedSourceError::ProcessingCancelled);
    }
    let fingerprint = load_lineage_fingerprint(case_conn, data_source_id)
        .map_err(|error| DerivedSourceError::Reconstruction(error.to_string()))?;
    if !catalog_phase_is_current(case_conn, data_source_id, &fingerprint)? {
        return Err(DerivedSourceError::InconsistentState(
            "ready derived source has a stale Catalog phase".to_string(),
        ));
    }
    let report = finalize_derived_source(
        case_conn,
        case_root,
        case_id,
        data_source_id,
        &fingerprint,
        &cancel_token,
    );
    log_finalization_report(data_source_id, &report);
    if cancel_token.load(Ordering::Relaxed) {
        return Err(DerivedSourceError::ProcessingCancelled);
    }
    if report.all_ready() {
        Ok(())
    } else {
        Err(DerivedSourceError::IncompleteProcessing {
            failed_count: report.failed_count(),
            deferred_count: report.deferred_count(),
            unfinished_count: report.unfinished_count(),
        })
    }
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

#[cfg(test)]
#[path = "../../../tests/unit/ceph_reconstruction/derived_source/materialization.rs"]
mod tests;

fn ensure_not_cancelled(cancel_token: &AtomicBool) -> DerivedSourceResult<()> {
    if cancel_token.load(Ordering::Relaxed) {
        Err(DerivedSourceError::ProcessingCancelled)
    } else {
        Ok(())
    }
}
