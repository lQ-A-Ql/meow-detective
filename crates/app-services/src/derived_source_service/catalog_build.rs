use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::time::Instant;

use domain::{CaseId, DataSource, DataSourceId};
use persistence_sqlite::repositories::catalog_publication_repo::CatalogPublicationRepo;

use crate::source_db;

use super::catalog_manifest::persist_current_source_manifest;
use super::filesystem::build_catalog_on_connection;
use super::{DerivedSourceError, DerivedSourceResult, MaterializedRbdSource};
use crate::ceph_reconstruction::{RadosReplicaSource, RbdImageDescriptor};
use crate::derived_source_service::finalizer::{refresh_catalog_claim, ProcessingPhaseAttempt};

pub(super) struct CatalogBuildRequest<'a> {
    pub(super) case_conn: &'a rusqlite::Connection,
    pub(super) case_root: &'a Path,
    pub(super) case_id: &'a CaseId,
    pub(super) data_source: &'a DataSource,
    pub(super) replicas: &'a [RadosReplicaSource],
    pub(super) descriptor: &'a RbdImageDescriptor,
    pub(super) lineage_fingerprint: &'a str,
    pub(super) catalog_attempt: &'a ProcessingPhaseAttempt,
    pub(super) cancel_token: &'a AtomicBool,
}

pub(super) fn build_and_enumerate_source(
    request: CatalogBuildRequest<'_>,
) -> DerivedSourceResult<MaterializedRbdSource> {
    super::ensure_not_cancelled(request.cancel_token)?;
    let materialization_started = Instant::now();
    let attempt_id = request.catalog_attempt.attempt_id();
    let source_conn = source_db::open_fresh_source_build_db(
        request.case_root,
        &request.data_source.id,
        attempt_id,
    )?;
    let build_result = (|| {
        let summary = build_catalog_on_connection(
            &source_conn,
            request.case_id,
            request.data_source,
            request.replicas,
            request.descriptor,
            request.lineage_fingerprint,
            request.cancel_token,
        )?;
        persist_current_source_manifest(&source_conn, request.lineage_fingerprint, &summary)?;
        super::ensure_not_cancelled(request.cancel_token)?;
        source_db::finalize_source_build_db(&source_conn)?;
        Ok(summary)
    })();
    drop(source_conn);

    let summary = match build_result {
        Ok(summary) => summary,
        Err(error) => {
            discard_failed_source_build(
                request.case_root,
                &request.data_source.id,
                attempt_id,
                &error,
            );
            return Err(error);
        }
    };
    if let Err(error) = publish_claimed_source_build(
        request.case_conn,
        request.case_root,
        &request.data_source.id,
        request.lineage_fingerprint,
        request.catalog_attempt,
        &summary.catalog_digest,
    ) {
        discard_failed_source_build(
            request.case_root,
            &request.data_source.id,
            attempt_id,
            &error,
        );
        return Err(error);
    }
    tracing::info!(
        data_source_id = %request.data_source.id.0,
        total_elapsed_ms = materialization_started.elapsed().as_millis(),
        "Ceph RBD derived source catalog materialization completed"
    );
    Ok(summary)
}

pub(super) fn publish_claimed_source_build(
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    data_source_id: &DataSourceId,
    lineage_fingerprint: &str,
    catalog_attempt: &ProcessingPhaseAttempt,
    catalog_digest: &str,
) -> DerivedSourceResult<std::path::PathBuf> {
    refresh_catalog_claim(
        case_conn,
        data_source_id,
        lineage_fingerprint,
        catalog_attempt,
    )?;
    let source_db_rel_path = source_db::canonical_source_db_rel_path(data_source_id);
    let catalog_fingerprint =
        super::catalog_manifest::catalog_fingerprint_for_source(lineage_fingerprint);
    let publication = CatalogPublicationRepo::new(case_conn).prepare(
        data_source_id,
        catalog_attempt.attempt_id(),
        &catalog_fingerprint,
        &source_db_rel_path,
        catalog_digest,
    )?;
    let published_path =
        source_db::publish_source_build_db(case_root, data_source_id, catalog_attempt.attempt_id())
            .map_err(DerivedSourceError::Database)?;
    CatalogPublicationRepo::new(case_conn)
        .mark_published(
            data_source_id,
            catalog_attempt.attempt_id(),
            &publication.seal,
        )
        .map_err(DerivedSourceError::Database)?;
    Ok(published_path)
}

fn discard_failed_source_build(
    case_root: &Path,
    data_source_id: &DataSourceId,
    attempt_id: &str,
    primary_error: &DerivedSourceError,
) {
    if let Err(cleanup_error) =
        source_db::discard_source_build_db(case_root, data_source_id, attempt_id)
    {
        tracing::warn!(
            data_source_id = %data_source_id.0,
            error = %cleanup_error,
            primary_error = %primary_error,
            "Failed to discard an unpublished RBD Catalog build database"
        );
    }
}
