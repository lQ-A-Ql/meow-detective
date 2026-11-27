use std::path::Path;

use domain::{CaseId, DataSource, DataSourceId};
use persistence_sqlite::repositories::{
    catalog_publication_repo::CatalogPublicationRepo,
    datasource_repo::DataSourceRepo,
    processing_phase_repo::{DataSourceProcessingPhaseRepo, ProcessingPhase},
};

use super::{
    publication::{publication_request, RbdPublicationAttempt, RbdPublicationValidator},
    ready_source_summary_if_current, record_catalog_failure, start_catalog,
};
use crate::derived_source_service::{
    catalog_manifest::{load_current_source_summary, verify_current_source_manifest_deep},
    finalizer::{catalog_phase_is_current, ProcessingPhaseAttempt},
    DerivedSourceError, DerivedSourceResult, MaterializedRbdSource,
};
use ceph_reconstruction::ceph_reconstruction::load_lineage_fingerprint;

pub(super) fn reuse_existing_catalog(
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
        if let Some(summary) =
            ready_source_summary_if_current(case_conn, case_root, existing.clone())?
        {
            return Ok(Some(summary));
        }
    }
    if !source_kernel::source_db::source_db_path(case_root, data_source_id).is_file() {
        if CatalogPublicationRepo::new(case_conn)
            .find(data_source_id)?
            .is_some()
        {
            let fingerprint = load_lineage_fingerprint(case_conn, data_source_id)
                .map_err(|error| DerivedSourceError::Reconstruction(error.to_string()))?;
            let attempt = start_catalog(case_conn, data_source_id, &fingerprint)?;
            let claim = DataSourceProcessingPhaseRepo::new(case_conn)
                .find(data_source_id, ProcessingPhase::Catalog)?
                .ok_or_else(|| {
                    DerivedSourceError::InconsistentState("Catalog claim is missing".into())
                })?;
            if claim.attempt_id.as_deref() != Some(attempt.attempt_id()) {
                return Err(DerivedSourceError::ProcessingBusy { phase: "catalog" });
            }
            persistence_source::publication::catalog_publication::restore_prepared_build(
                case_conn,
                case_root,
                data_source_id,
                &claim,
            )?;
            return recover_catalog(case_conn, case_root, case_id, existing, Some(&attempt));
        }
        return Ok(None);
    }
    if let Some(summary) = recover_persisted_catalog(case_conn, case_root, case_id, existing)? {
        return Ok(Some(summary));
    }
    Err(DerivedSourceError::InconsistentState(format!(
        "persisted Catalog for {} is not reusable; automatic destructive reset is disabled",
        data_source_id.0
    )))
}

pub(super) fn recover_persisted_catalog(
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    case_id: &CaseId,
    data_source: DataSource,
) -> DerivedSourceResult<Option<MaterializedRbdSource>> {
    recover_catalog(case_conn, case_root, case_id, data_source, None)
}

fn recover_catalog(
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    case_id: &CaseId,
    data_source: DataSource,
    existing_attempt: Option<&ProcessingPhaseAttempt>,
) -> DerivedSourceResult<Option<MaterializedRbdSource>> {
    let data_source_id = data_source.id.clone();
    let lineage_fingerprint = load_lineage_fingerprint(case_conn, &data_source_id)
        .map_err(|error| DerivedSourceError::Reconstruction(error.to_string()))?;
    let source_connection = source_kernel::source_db::open_catalog_recovery_source_by_id(
        case_conn,
        case_root,
        case_id,
        &data_source_id,
    )
    .map_err(|error| DerivedSourceError::Database(error.into_db_error()))?;
    source_kernel::source_db::verify_source_db_integrity(&source_connection)?;
    source_kernel::source_db::checkpoint_source_db(&source_connection)?;
    let Some(summary) = load_current_source_summary(
        &source_connection,
        &lineage_fingerprint,
        data_source.clone(),
    )?
    else {
        return Ok(None);
    };
    validate_recovery_catalog(&source_connection, &lineage_fingerprint, data_source)?;
    let fingerprint =
        crate::derived_source_service::catalog_manifest::catalog_fingerprint_for_source(
            &lineage_fingerprint,
        );
    if catalog_phase_is_current(case_conn, &data_source_id, &lineage_fingerprint)? {
        let validator = RbdPublicationValidator { case_root };
        persistence_source::publication::catalog_publication::verify_publication(
            case_conn,
            case_root,
            &summary.data_source,
            &fingerprint,
            case_id,
            persistence_source::publication::catalog_publication::CatalogProjection::Partition,
            &validator,
        )?;
        return Ok(Some(summary));
    }

    let acquired;
    let attempt = match existing_attempt {
        Some(attempt) => attempt,
        None => {
            acquired = start_catalog(case_conn, &data_source_id, &lineage_fingerprint)?;
            &acquired
        }
    };
    let validator = RbdPublicationValidator { case_root };
    let publication_attempt = RbdPublicationAttempt {
        attempt,
        summary: &summary,
        lineage_fingerprint: &lineage_fingerprint,
    };
    let cancel = std::sync::atomic::AtomicBool::new(false);
    let request = publication_request(
        case_conn,
        case_root,
        case_id,
        &summary.data_source,
        &fingerprint,
        &publication_attempt,
        &validator,
        &cancel,
    );
    match persistence_source::publication::catalog_publication::recover_prepared_publication(
        &request,
    ) {
        Ok(true) => {}
        Ok(false) => {
            return Err(DerivedSourceError::InconsistentState(format!(
                "persisted Catalog for {} disappeared during recovery",
                data_source_id.0
            )))
        }
        Err(error) => {
            let error: DerivedSourceError = error.into();
            record_catalog_failure(
                case_conn,
                &data_source_id,
                &lineage_fingerprint,
                attempt,
                &error,
            );
            return Err(error);
        }
    }
    tracing::info!(
        data_source_id = %data_source_id.0,
        "Recovered a verified RBD Catalog without re-enumerating the filesystem"
    );
    Ok(Some(summary))
}

fn validate_recovery_catalog(
    conn: &rusqlite::Connection,
    fingerprint: &str,
    source: DataSource,
) -> DerivedSourceResult<()> {
    let source_id = source.id.clone();
    if !verify_current_source_manifest_deep(conn, fingerprint, source)? {
        return Err(DerivedSourceError::InconsistentState(format!(
            "persisted Catalog for {} does not match its source database",
            source_id.0,
        )));
    }
    Ok(())
}
