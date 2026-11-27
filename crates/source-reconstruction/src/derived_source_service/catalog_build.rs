use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::time::Instant;

use domain::{CaseId, DataSource, DataSourceId};
use persistence_source::publication::catalog_publication::CatalogProjection;

use source_kernel::source_db;

use super::catalog_manifest::persist_current_source_manifest;
use super::filesystem::build_catalog_on_connection;
use super::{DerivedSourceError, DerivedSourceResult, MaterializedRbdSource};
use crate::derived_source_service::finalizer::ProcessingPhaseAttempt;
use ceph_reconstruction::ceph_reconstruction::{
    RadosReplicaSource, RbdImageDescriptor, RbdReplicaPolicy,
};

pub(super) struct CatalogBuildRequest<'a> {
    pub(super) case_conn: &'a rusqlite::Connection,
    pub(super) case_root: &'a Path,
    pub(super) case_id: &'a CaseId,
    pub(super) data_source: &'a DataSource,
    pub(super) replicas: &'a [RadosReplicaSource],
    pub(super) policy: &'a RbdReplicaPolicy,
    pub(super) descriptor: &'a RbdImageDescriptor,
    pub(super) lineage_fingerprint: &'a str,
    pub(super) catalog_attempt: &'a ProcessingPhaseAttempt,
    pub(super) cancel_token: &'a AtomicBool,
}

pub(super) struct RbdCatalogBuildContext<'a> {
    pub(super) source_conn: &'a rusqlite::Connection,
    pub(super) fjall_source: &'a source_kernel::storage_runtime::ReadWriteSourceStorageHandle,
    pub(super) case_id: &'a CaseId,
    pub(super) data_source: &'a DataSource,
    pub(super) replicas: &'a [RadosReplicaSource],
    pub(super) policy: &'a RbdReplicaPolicy,
    pub(super) descriptor: &'a RbdImageDescriptor,
    pub(super) lineage_fingerprint: &'a str,
    pub(super) cancel_token: &'a AtomicBool,
}

pub(super) fn build_and_enumerate_source(
    request: CatalogBuildRequest<'_>,
) -> DerivedSourceResult<(MaterializedRbdSource, String)> {
    super::ensure_not_cancelled(request.cancel_token)?;
    let materialization_started = Instant::now();
    let attempt_id = request.catalog_attempt.attempt_id();
    let source_conn = source_db::open_fresh_source_build_db(
        request.case_root,
        &request.data_source.id,
        attempt_id,
    )?;
    let fjall_source = prepare_fjall_source(&request, &source_conn)?;
    let summary = match build_catalog_on_connection(RbdCatalogBuildContext {
        source_conn: &source_conn,
        fjall_source: &fjall_source,
        case_id: request.case_id,
        data_source: request.data_source,
        replicas: request.replicas,
        policy: request.policy,
        descriptor: request.descriptor,
        lineage_fingerprint: request.lineage_fingerprint,
        cancel_token: request.cancel_token,
    }) {
        Ok(summary) => summary,
        Err(error) => {
            drop(fjall_source);
            drop(source_conn);
            discard_failed_source_build(
                request.case_conn,
                request.case_root,
                &request.data_source.id,
                attempt_id,
                &error,
            );
            return Err(error);
        }
    };
    let publication_digest =
        match finalize_source_generation(source_conn, fjall_source, &request, &summary) {
            Ok(digest) => digest,
            Err(error) => {
                discard_failed_source_build(
                    request.case_conn,
                    request.case_root,
                    &request.data_source.id,
                    attempt_id,
                    &error,
                );
                return Err(error);
            }
        };
    tracing::info!(
        data_source_id = %request.data_source.id.0,
        total_elapsed_ms = materialization_started.elapsed().as_millis(),
        "Ceph RBD derived source catalog materialization completed"
    );
    Ok((summary, publication_digest))
}

fn finalize_source_generation(
    source_conn: rusqlite::Connection,
    fjall_source: source_kernel::storage_runtime::ReadWriteSourceStorageHandle,
    request: &CatalogBuildRequest<'_>,
    summary: &MaterializedRbdSource,
) -> DerivedSourceResult<String> {
    fjall_source
        .persist(persistence_source::fjall::PersistMode::SyncAll)
        .map_err(|error| DerivedSourceError::Reconstruction(error.to_string()))?;
    persist_current_source_manifest(
        &source_conn,
        &fjall_source,
        request.lineage_fingerprint,
        summary,
    )?;
    super::ensure_not_cancelled(request.cancel_token)?;
    let publication_digest =
        persistence_source::publication::catalog_publication::seal_catalog_generation(
            &source_conn,
            &request.data_source.id,
            request.catalog_attempt.attempt_id(),
            &super::catalog_manifest::catalog_fingerprint_for_source(request.lineage_fingerprint),
            CatalogProjection::Partition,
            request.cancel_token,
        )?;
    source_db::finalize_source_build_db(&source_conn)?;
    Ok(publication_digest)
}

fn prepare_fjall_source(
    request: &CatalogBuildRequest<'_>,
    conn: &rusqlite::Connection,
) -> DerivedSourceResult<source_kernel::storage_runtime::ReadWriteSourceStorageHandle> {
    let layout = persistence_source::fjall::SourceBuildLayout::for_case_source(
        request.case_root,
        &request.data_source.id.0,
        request.catalog_attempt.attempt_id(),
    )
    .map_err(|error| DerivedSourceError::Reconstruction(error.to_string()))?;
    let root = layout.build_path();
    if layout.published_path().exists() {
        return Err(DerivedSourceError::InconsistentState(
            "published RBD generation requires publication recovery".into(),
        ));
    }
    let name = root
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            DerivedSourceError::InconsistentState("source generation component is missing".into())
        })?;
    persistence_source::detail::bind_generation(conn, name)?;
    std::fs::create_dir_all(&root)?;
    let source = source_kernel::storage_runtime::open_source_read_write_at_path(&root)
        .map_err(|error| DerivedSourceError::Reconstruction(error.to_string()))?;
    Ok(source)
}

fn discard_failed_source_build(
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    data_source_id: &DataSourceId,
    attempt_id: &str,
    primary_error: &DerivedSourceError,
) {
    if let Err(cleanup_error) =
        persistence_source::publication::catalog_publication::discard_unpublished_catalog_build(
            case_conn,
            case_root,
            data_source_id,
            attempt_id,
        )
    {
        tracing::warn!(
            data_source_id = %data_source_id.0,
            error = %cleanup_error,
            primary_error = %primary_error,
            "Failed to discard an unpublished RBD Catalog build database"
        );
    }
}
