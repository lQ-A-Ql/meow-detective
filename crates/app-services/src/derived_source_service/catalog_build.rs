use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::time::Instant;

use domain::{CaseId, DataSource, DataSourceId};
use persistence_sqlite::repositories::catalog_publication_repo::CatalogPublicationRepo;

use crate::source_db;

use super::catalog_manifest::persist_current_source_manifest;
use super::filesystem::build_catalog_on_connection;
use super::{DerivedSourceError, DerivedSourceResult, MaterializedRbdSource};
use crate::ceph_reconstruction::{RadosReplicaSource, RbdImageDescriptor, RbdReplicaPolicy};
use crate::derived_source_service::finalizer::{refresh_catalog_claim, ProcessingPhaseAttempt};

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
    pub(super) fjall_source: &'a crate::storage_runtime::ReadWriteSourceStorageHandle,
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
) -> DerivedSourceResult<MaterializedRbdSource> {
    super::ensure_not_cancelled(request.cancel_token)?;
    let materialization_started = Instant::now();
    let attempt_id = request.catalog_attempt.attempt_id();
    let source_conn = source_db::open_fresh_source_build_db(
        request.case_root,
        &request.data_source.id,
        attempt_id,
    )?;
    let FjallBuildStorage {
        layout: fjall_layout,
        root: fjall_target_root,
        existing: fjall_generation_exists,
        source: fjall_source,
    } = prepare_fjall_source(&request, &source_conn)?;
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
                request.case_root,
                &request.data_source.id,
                attempt_id,
                &error,
            );
            return Err(error);
        }
    };
    fjall_source
        .persist(persistence_source::fjall::PersistMode::SyncAll)
        .map_err(|error| DerivedSourceError::Reconstruction(error.to_string()))?;
    let source_finalize = (|| {
        persist_current_source_manifest(
            &source_conn,
            &fjall_source,
            request.lineage_fingerprint,
            &summary,
        )?;
        super::ensure_not_cancelled(request.cancel_token)?;
        source_db::finalize_source_build_db(&source_conn)?;
        Ok::<_, DerivedSourceError>(())
    })();
    drop(fjall_source);
    drop(source_conn);
    if let Err(error) = source_finalize {
        discard_failed_source_build(
            request.case_root,
            &request.data_source.id,
            attempt_id,
            &error,
        );
        return Err(error);
    }
    publish_fjall_partition_generation(
        &request.data_source.id,
        attempt_id,
        &fjall_layout,
        &fjall_target_root,
        fjall_generation_exists,
        &summary.catalog_digest,
    )?;
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

struct FjallBuildStorage {
    layout: persistence_source::fjall::SourceBuildLayout,
    root: std::path::PathBuf,
    existing: bool,
    source: crate::storage_runtime::ReadWriteSourceStorageHandle,
}

fn prepare_fjall_source(
    request: &CatalogBuildRequest<'_>,
    conn: &rusqlite::Connection,
) -> DerivedSourceResult<FjallBuildStorage> {
    let layout = persistence_source::fjall::SourceBuildLayout::for_case_source(
        request.case_root,
        &request.data_source.id.0,
        request.catalog_attempt.attempt_id(),
    )
    .map_err(|error| DerivedSourceError::Reconstruction(error.to_string()))?;
    let existing = layout.published_path().is_dir();
    let root = if existing {
        layout.published_path()
    } else {
        layout.build_path()
    };
    let name = root
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            DerivedSourceError::InconsistentState("source generation component is missing".into())
        })?;
    persistence_source::detail::bind_generation(conn, name)?;
    std::fs::create_dir_all(&root)?;
    let source = crate::storage_runtime::open_source_read_write_at_path(&root)
        .map_err(|error| DerivedSourceError::Reconstruction(error.to_string()))?;
    if existing {
        crate::import_pipeline::begin_partition_projection(
            request.case_root,
            &request.data_source.id,
        )
        .map_err(|error| DerivedSourceError::Reconstruction(error.to_string()))?;
    }
    Ok(FjallBuildStorage {
        layout,
        root,
        existing,
        source,
    })
}

fn publish_fjall_partition_generation(
    data_source_id: &DataSourceId,
    attempt_id: &str,
    layout: &persistence_source::fjall::SourceBuildLayout,
    target_root: &std::path::Path,
    replace_published: bool,
    catalog_digest: &str,
) -> DerivedSourceResult<()> {
    let _close_guard = crate::storage_runtime::close_source_store_at_path(target_root)
        .map_err(|error| DerivedSourceError::Reconstruction(error.to_string()))?
        .ok_or_else(|| {
            DerivedSourceError::Reconstruction("Fjall source build is still in use".to_owned())
        })?;
    let source = persistence_source::fjall::SourceStore::open(
        target_root,
        persistence_source::fjall::DatabaseConfig::default(),
    )
    .map_err(|error| DerivedSourceError::Reconstruction(error.to_string()))?;
    let projection_digest =
        hex::encode(persistence_source::fjall::digest(catalog_digest.as_bytes()));
    let seal = if replace_published {
        persistence_source::fjall::replace_source_generation(
            &source,
            &data_source_id.0,
            attempt_id,
            &projection_digest,
        )
    } else {
        persistence_source::fjall::seal_source_generation(
            &source,
            &data_source_id.0,
            attempt_id,
            &projection_digest,
        )
    };
    seal.map_err(|error| DerivedSourceError::Reconstruction(error.to_string()))?;
    let (keyspace, key, marker) =
        crate::partition_storage_service::partition_projection_marker(&data_source_id.0)
            .map_err(|error| DerivedSourceError::Reconstruction(error.to_string()))?;
    source
        .write_envelope(
            keyspace,
            key,
            &marker,
            persistence_source::fjall::PersistMode::SyncAll,
        )
        .map_err(|error| DerivedSourceError::Reconstruction(error.to_string()))?;
    source
        .database()
        .persist(persistence_source::fjall::PersistMode::SyncAll)
        .map_err(|error| DerivedSourceError::Reconstruction(error.to_string()))?;
    drop(source);
    if !replace_published {
        persistence_source::fjall::publish_source_generation(target_root, layout.published_path())
            .map_err(|error| DerivedSourceError::Reconstruction(error.to_string()))?;
    }
    drop(_close_guard);
    Ok(())
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
