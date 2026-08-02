use std::path::Path;

use domain::{CaseId, DataSource, DataSourceId};
use persistence_sqlite::repositories::{
    catalog_publication_repo::CatalogPublicationRepo, datasource_repo::DataSourceRepo,
};

use super::{
    publish_catalog_readiness, ready_source_summary_if_current, record_catalog_failure,
    start_catalog,
};
use crate::ceph_reconstruction::load_lineage_fingerprint;
use crate::derived_source_service::{
    catalog_manifest::{load_current_source_summary, verify_current_source_manifest_deep},
    finalizer::{catalog_phase_is_current, queue_post_catalog_phases},
    DerivedSourceError, DerivedSourceResult, MaterializedRbdSource,
};

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
    if !crate::source_db::source_db_path(case_root, data_source_id).is_file() {
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
    let data_source_id = data_source.id.clone();
    let lineage_fingerprint = load_lineage_fingerprint(case_conn, &data_source_id)
        .map_err(|error| DerivedSourceError::Reconstruction(error.to_string()))?;
    let source_connection = crate::source_db::open_catalog_recovery_source_by_id(
        case_conn,
        case_root,
        case_id,
        &data_source_id,
    )
    .map_err(|error| DerivedSourceError::Database(error.into_db_error()))?;
    crate::source_db::verify_source_db_integrity(&source_connection)?;
    crate::source_db::checkpoint_source_db(&source_connection)?;
    let Some(summary) = load_current_source_summary(
        &source_connection,
        &lineage_fingerprint,
        data_source.clone(),
    )?
    else {
        return Ok(None);
    };
    if !verify_current_source_manifest_deep(&source_connection, &lineage_fingerprint, data_source)?
    {
        return Err(DerivedSourceError::InconsistentState(format!(
            "persisted Catalog for {} does not match its source database",
            data_source_id.0
        )));
    }
    reconcile_catalog_publication(case_conn, &data_source_id, &lineage_fingerprint, &summary)?;

    if catalog_phase_is_current(case_conn, &data_source_id, &lineage_fingerprint)? {
        publish_recovered_catalog_registration(case_conn, &data_source_id, &lineage_fingerprint)?;
        return Ok(Some(summary));
    }

    let attempt = start_catalog(case_conn, &data_source_id, &lineage_fingerprint)?;
    if let Err(error) = publish_catalog_readiness(
        case_conn,
        &summary.data_source,
        &lineage_fingerprint,
        &attempt,
        &summary,
    ) {
        record_catalog_failure(
            case_conn,
            &data_source_id,
            &lineage_fingerprint,
            &attempt,
            &error,
        );
        return Err(error);
    }
    tracing::info!(
        data_source_id = %data_source_id.0,
        "Recovered a verified RBD Catalog without re-enumerating the filesystem"
    );
    Ok(Some(summary))
}

fn reconcile_catalog_publication(
    case_conn: &rusqlite::Connection,
    data_source_id: &DataSourceId,
    lineage_fingerprint: &str,
    summary: &MaterializedRbdSource,
) -> DerivedSourceResult<()> {
    let publication = CatalogPublicationRepo::new(case_conn)
        .find(data_source_id)?
        .ok_or_else(|| {
            DerivedSourceError::InconsistentState(format!(
                "persisted Catalog for {} has no app database publication seal",
                data_source_id.0
            ))
        })?;
    let catalog_fingerprint =
        crate::derived_source_catalog::catalog_fingerprint(lineage_fingerprint);
    let source_db_rel_path = crate::source_db::canonical_source_db_rel_path(data_source_id);
    let expected_seal = persistence_sqlite::repositories::catalog_publication_repo::seal_for(
        &data_source_id.0,
        &publication.attempt_id,
        &catalog_fingerprint,
        &source_db_rel_path,
        &summary.catalog_digest,
    );
    if publication.input_fingerprint != catalog_fingerprint
        || publication.source_db_rel_path != source_db_rel_path
        || publication.catalog_digest != summary.catalog_digest
        || publication.seal != expected_seal
    {
        return Err(DerivedSourceError::InconsistentState(format!(
            "persisted Catalog publication seal for {} does not match its source database",
            data_source_id.0
        )));
    }
    match publication.state.as_str() {
        "prepared" => {
            CatalogPublicationRepo::new(case_conn).mark_published(
                data_source_id,
                &publication.attempt_id,
                &publication.seal,
            )?;
        }
        "published" => {}
        state => {
            return Err(DerivedSourceError::InconsistentState(format!(
                "persisted Catalog publication for {} has unsupported state '{}'",
                data_source_id.0, state
            )));
        }
    }
    Ok(())
}

fn publish_recovered_catalog_registration(
    case_conn: &rusqlite::Connection,
    data_source_id: &DataSourceId,
    fingerprint: &str,
) -> DerivedSourceResult<()> {
    let transaction = case_conn
        .unchecked_transaction()
        .map_err(persistence_sqlite::DbError::from)?;
    queue_post_catalog_phases(&transaction, data_source_id, fingerprint)?;
    DataSourceRepo::new(&transaction).update_import_state(data_source_id, "ready", None)?;
    transaction
        .commit()
        .map_err(persistence_sqlite::DbError::from)?;
    Ok(())
}
