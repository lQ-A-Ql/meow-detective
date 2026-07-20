use std::path::Path;

use domain::DataSource;
use persistence_sqlite::repositories::{
    catalog_publication_repo::{seal_for, CatalogPublicationRepo},
    ceph_fs_lineage_repo::CephFsDerivedLineageAggregate,
    ceph_fs_namespace_repo::CephFsNamespaceRepo,
    datasource_repo::DataSourceRepo,
};

use super::{
    catalog::{catalog_input_fingerprint, claim_catalog, complete_catalog, CatalogClaim},
    materialization::{load_ready_summary, read_capability_record, validate_assembly_record},
    CephFsSourceError, CephFsSourceResult, MaterializedCephFsSource,
};

pub(super) fn recover_published_source(
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    source: &DataSource,
    lineage: &CephFsDerivedLineageAggregate,
) -> CephFsSourceResult<Option<MaterializedCephFsSource>> {
    let source_path = crate::source_db::source_db_path(case_root, &source.id);
    if !source_path.is_file() {
        return Ok(None);
    }
    let Some(connection) = validate_published_database(case_root, case_conn, source, lineage)?
    else {
        // An incomplete namespace is intentionally retained for diagnostics,
        // but it is not a recoverable publication.  Keep the source visible as
        // failed so the caller can require an explicit delete/re-import.
        return Ok(None);
    };
    let Some(publication) = CatalogPublicationRepo::new(case_conn).find(&source.id)? else {
        return Err(CephFsSourceError::StalePublication);
    };
    let catalog_fingerprint = catalog_input_fingerprint(&lineage.lineage.lineage_fingerprint);
    let expected_rel_path = crate::source_db::canonical_source_db_rel_path(&source.id);
    let expected_seal = seal_for(
        &source.id.0,
        &publication.attempt_id,
        &catalog_fingerprint,
        &expected_rel_path,
        &lineage.lineage.namespace_projection_sha256,
    );
    if publication.input_fingerprint != catalog_fingerprint
        || publication.source_db_rel_path != expected_rel_path
        || publication.catalog_digest != lineage.lineage.namespace_projection_sha256
        || publication.seal != expected_seal
    {
        return Err(CephFsSourceError::StalePublication);
    }
    if publication.state == "prepared" {
        CatalogPublicationRepo::new(case_conn).mark_published(
            &source.id,
            &publication.attempt_id,
            &publication.seal,
        )?;
    } else if publication.state != "published" {
        return Err(CephFsSourceError::StalePublication);
    }
    drop(connection);
    finish_recovered_catalog(
        case_conn,
        case_root,
        source,
        &lineage.lineage.lineage_fingerprint,
    )?;
    load_ready_summary(
        case_conn,
        case_root,
        source,
        &lineage.lineage.lineage_fingerprint,
    )
    .map(Some)
}

fn validate_published_database(
    case_root: &Path,
    case_conn: &rusqlite::Connection,
    source: &DataSource,
    lineage: &CephFsDerivedLineageAggregate,
) -> CephFsSourceResult<Option<rusqlite::Connection>> {
    let path = crate::source_db::registered_source_db_path(case_conn, case_root, &source.id)?;
    crate::source_db::verify_finalized_source_db(&path, &source.id)?;
    let connection = persistence_sqlite::open_existing_source_read_only(&path)?;
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

fn finish_recovered_catalog(
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    source: &DataSource,
    lineage_fingerprint: &str,
) -> CephFsSourceResult<()> {
    let catalog_fingerprint = catalog_input_fingerprint(lineage_fingerprint);
    match claim_catalog(case_conn, &source.id, &catalog_fingerprint)? {
        CatalogClaim::Ready => {
            DataSourceRepo::new(case_conn).update_import_state(&source.id, "ready", None)?;
        }
        CatalogClaim::Acquired(attempt) => {
            let summary = load_ready_summary(case_conn, case_root, source, lineage_fingerprint)?;
            let transaction = case_conn
                .unchecked_transaction()
                .map_err(persistence_sqlite::DbError::from)?;
            complete_catalog(&transaction, &source.id, &attempt, &summary)?;
            DataSourceRepo::new(&transaction).update_import_state(&source.id, "ready", None)?;
            transaction
                .commit()
                .map_err(persistence_sqlite::DbError::from)?;
        }
    }
    Ok(())
}
