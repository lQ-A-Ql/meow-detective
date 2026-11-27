use std::path::Path;

use domain::DataSource;
use persistence_source::repositories::ceph_fs_namespace_repo::CephFsNamespaceRepo;
use persistence_sqlite::repositories::{
    ceph_fs_lineage_repo::CephFsDerivedLineageAggregate,
    datasource_repo::DataSourceRepo,
    processing_phase_repo::{DataSourceProcessingPhaseRepo, ProcessingPhase},
};

use super::{
    catalog::{catalog_input_fingerprint, claim_catalog, CatalogAttempt, CatalogClaim},
    materialization::{load_published_summary, read_capability_record, validate_assembly_record},
    publication::{
        map_error, publication_request, CephFsPublicationAttempt, CephFsPublicationValidator,
    },
    CephFsSourceError, CephFsSourceResult, MaterializedCephFsSource,
};

pub(super) fn recover_published_source(
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    source: &DataSource,
    lineage: &CephFsDerivedLineageAggregate,
    cancel: &std::sync::atomic::AtomicBool,
) -> CephFsSourceResult<Option<MaterializedCephFsSource>> {
    let source_path = source_kernel::source_db::source_db_path(case_root, &source.id);
    let fingerprint = catalog_input_fingerprint(&lineage.lineage.lineage_fingerprint);
    let restored = if !source_path.is_file() {
        Some(restore_catalog_files(
            case_conn,
            case_root,
            source,
            &fingerprint,
        )?)
    } else {
        None
    };
    let Some(connection) = validate_published_database(case_root, case_conn, source, lineage)?
    else {
        return Ok(None);
    };
    drop(connection);
    let summary = load_published_summary(case_conn, case_root, source, lineage)?;
    let validator = CephFsPublicationValidator { case_root, lineage };
    let case_id = DataSourceRepo::new(case_conn).case_id(&source.id)?;
    let claim = match restored {
        Some(attempt) => CatalogClaim::Acquired(attempt),
        None => claim_catalog(case_conn, &source.id, &fingerprint)?,
    };
    match claim {
        CatalogClaim::Ready => {
            persistence_source::publication::catalog_publication::verify_publication(
                case_conn,
                case_root,
                source,
                &fingerprint,
                &case_id,
                persistence_source::publication::catalog_publication::CatalogProjection::SourceOnly,
                &validator,
            )
            .map_err(map_error)?;
            DataSourceRepo::new(case_conn).update_import_state(&source.id, "ready", None)?;
        }
        CatalogClaim::Acquired(attempt) => {
            let publication_attempt = CephFsPublicationAttempt {
                attempt: &attempt,
                summary: &summary,
            };
            let request = publication_request(
                case_conn,
                case_root,
                &case_id,
                source,
                &fingerprint,
                &publication_attempt,
                &validator,
                cancel,
            );
            match persistence_source::publication::catalog_publication::recover_prepared_publication(
                &request,
            ) {
                Ok(true) => {}
                Ok(false) => {
                    return Err(CephFsSourceError::InconsistentState(
                        "published CephFS source disappeared during recovery".to_string(),
                    ))
                }
                Err(error) => return Err(map_error(error)),
            }
        }
    }
    Ok(Some(summary))
}

fn restore_catalog_files(
    conn: &rusqlite::Connection,
    case_root: &Path,
    source: &DataSource,
    fingerprint: &str,
) -> CephFsSourceResult<CatalogAttempt> {
    let attempt = match claim_catalog(conn, &source.id, fingerprint)? {
        CatalogClaim::Acquired(attempt) => attempt,
        CatalogClaim::Ready => return Err(CephFsSourceError::StalePublication),
    };
    let claim = DataSourceProcessingPhaseRepo::new(conn)
        .find(&source.id, ProcessingPhase::Catalog)?
        .ok_or(CephFsSourceError::StalePublication)?;
    if claim.attempt_id.as_deref() != Some(attempt.attempt_id.as_str()) {
        return Err(CephFsSourceError::ProcessingBusy);
    }
    persistence_source::publication::catalog_publication::restore_prepared_build(
        conn, case_root, &source.id, &claim,
    )
    .map_err(map_error)?;
    Ok(attempt)
}

pub(super) fn validate_published_database(
    case_root: &Path,
    case_conn: &rusqlite::Connection,
    source: &DataSource,
    lineage: &CephFsDerivedLineageAggregate,
) -> CephFsSourceResult<Option<rusqlite::Connection>> {
    let path =
        source_kernel::source_db::registered_source_db_path(case_conn, case_root, &source.id)?;
    source_kernel::source_db::verify_finalized_source_db(&path, &source.id)?;
    let connection = persistence_source::control::opening::open_source_database_read_only(&path)?;
    let Some(manifest) = CephFsNamespaceRepo::new(&connection)
        .find_manifest(&lineage.lineage.filesystem_identity, &source.id.0)?
    else {
        return Err(CephFsSourceError::InconsistentState(
            "CephFS source database has no namespace manifest".to_string(),
        ));
    };
    if !manifest.published || manifest.completeness != "closed" {
        return Ok(None);
    }
    let published = CephFsNamespaceRepo::new(&connection).verify_published_catalog(
        &lineage.lineage.filesystem_identity,
        &source.id.0,
        &source.name,
    )?;
    let manifest = published.manifest;
    if manifest.input_sha256 != lineage.lineage.namespace_input_sha256
        || manifest.projection_sha256 != lineage.lineage.namespace_projection_sha256
    {
        return Err(CephFsSourceError::StalePublication);
    }
    validate_assembly_record(&connection, &lineage.lineage, &source.id.0, true)?;
    read_capability_record(&connection, &lineage.lineage, &source.id.0)?;
    Ok(Some(connection))
}
