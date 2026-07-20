use persistence_sqlite::repositories::{
    catalog_publication_repo::CatalogPublicationRepo,
    datasource_cluster_repo::DataSourceClusterRepo, datasource_repo::DataSourceRepo,
};

use super::{
    capability::derive_source_capability,
    catalog::{
        catalog_input_fingerprint, claim_catalog, complete_catalog, defer_incomplete_catalog,
        fail_catalog, refresh_catalog, CatalogClaim,
    },
    lineage::{
        build_data_source, build_lineage, derived_source_id, source_storage, CephFsLineageEvidence,
    },
    presence_gate::validate_presence,
    projection::{build_file_entries, build_namespace_projection},
    recovery::recover_published_source,
    registration::{ensure_registration, import_state},
    source_build::{build_source_database, CephFsSourceBuildRequest},
    CephFsSourceError, CephFsSourceMaterializationRequest, CephFsSourceResult,
    MaterializedCephFsSource,
};

pub fn materialize_cephfs_source(
    request: CephFsSourceMaterializationRequest<'_>,
) -> CephFsSourceResult<MaterializedCephFsSource> {
    validate_cluster(&request)?;
    validate_presence(request.presence, request.descriptor)?;
    let namespace = ceph_wire::assemble_cephfs_namespace(request.namespace_assembly_input.clone())?;
    let data_source_id = derived_source_id(request.cluster_id, &request.descriptor.identity)?;
    let projection = build_namespace_projection(
        &data_source_id,
        request.descriptor,
        &namespace,
        request.namespace_input_sha256,
        request.inline_data_by_inode,
        request.sparse_extents_by_inode,
    )?;
    let desired_source = build_data_source(request.cluster_id, &data_source_id, request.descriptor);
    let storage = source_storage(&data_source_id);
    let capability = derive_source_capability(&namespace, &projection);
    let lineage = build_lineage(
        &data_source_id,
        request.cluster_id,
        request.descriptor,
        CephFsLineageEvidence {
            namespace_input_sha256: request.namespace_input_sha256,
            namespace_projection_sha256: &projection.manifest.projection_sha256,
            namespace_assembly_sha256: namespace.assembly_sha256(),
            source_capability: capability,
            journal_boundary_sha256: request.journal_boundary_sha256,
            expected_replica_count: request.expected_replica_count,
        },
    )?;
    let source = ensure_registration(
        request.case_conn,
        request.case_id,
        &desired_source,
        &storage,
        &lineage,
    )?;
    if import_state(request.case_conn, &data_source_id)? == "ready" {
        return load_ready_summary(
            request.case_conn,
            request.case_root,
            &source,
            &lineage.lineage.lineage_fingerprint,
        );
    }
    if crate::source_db::source_db_path(request.case_root, &data_source_id).exists() {
        if let Some(summary) =
            recover_published_source(request.case_conn, request.case_root, &source, &lineage)?
        {
            return Ok(summary);
        }
        return Err(CephFsSourceError::RetainedIncompleteSource);
    }
    let catalog_fingerprint = catalog_input_fingerprint(&lineage.lineage.lineage_fingerprint);
    let attempt = match claim_catalog(request.case_conn, &data_source_id, &catalog_fingerprint)? {
        CatalogClaim::Acquired(attempt) => attempt,
        CatalogClaim::Ready => {
            return load_ready_summary(
                request.case_conn,
                request.case_root,
                &source,
                &lineage.lineage.lineage_fingerprint,
            )
        }
    };
    let result = run_catalog_attempt(
        &request,
        CatalogRun {
            data_source_id: &data_source_id,
            source: &source,
            attempt: &attempt,
            projection: &projection,
            namespace: &namespace,
            lineage_fingerprint: &lineage.lineage.lineage_fingerprint,
            capability,
        },
    );
    if let Err(error) = &result {
        record_catalog_failure(&request, &data_source_id, &attempt, error);
    }
    result
}

struct CatalogRun<'a> {
    data_source_id: &'a domain::DataSourceId,
    source: &'a domain::DataSource,
    attempt: &'a super::catalog::CatalogAttempt,
    projection:
        &'a persistence_sqlite::repositories::ceph_fs_namespace_repo::CephFsNamespaceProjection,
    namespace: &'a ceph_wire::CephFsNamespaceAssembly,
    lineage_fingerprint: &'a str,
    capability: super::CephFsSourceCapability,
}

fn run_catalog_attempt(
    request: &CephFsSourceMaterializationRequest<'_>,
    run: CatalogRun<'_>,
) -> CephFsSourceResult<MaterializedCephFsSource> {
    let CatalogRun {
        data_source_id,
        source,
        attempt,
        projection,
        namespace,
        lineage_fingerprint,
        capability,
    } = run;
    let file_entries = if projection.manifest.published {
        build_file_entries(data_source_id, &source.name, namespace.graph())?
    } else {
        Vec::new()
    };
    let result = build_source_database(CephFsSourceBuildRequest {
        case_root: request.case_root,
        case_id: request.case_id,
        source,
        attempt_id: &attempt.attempt_id,
        projection,
        file_entries: &file_entries,
        assembly: namespace,
        capability,
        lineage_fingerprint,
    })?;
    refresh_catalog(request.case_conn, data_source_id, attempt)?;
    if result.published {
        publish_complete(request, attempt, &result)
    } else {
        preserve_incomplete(request, attempt, &result)
    }
}

fn record_catalog_failure(
    request: &CephFsSourceMaterializationRequest<'_>,
    data_source_id: &domain::DataSourceId,
    attempt: &super::catalog::CatalogAttempt,
    error: &CephFsSourceError,
) {
    fail_catalog(request.case_conn, data_source_id, attempt, error);
    let _ = DataSourceRepo::new(request.case_conn).update_import_state(
        data_source_id,
        "failed",
        Some(&error.to_string()),
    );
    if let Err(cleanup_error) = crate::source_db::discard_source_build_db(
        request.case_root,
        data_source_id,
        &attempt.attempt_id,
    ) {
        tracing::warn!(
            data_source_id = %data_source_id.0,
            error = %cleanup_error,
            primary_error = %error,
            "Failed to discard CephFS source build database"
        );
    }
}

fn publish_complete(
    request: &CephFsSourceMaterializationRequest<'_>,
    attempt: &super::catalog::CatalogAttempt,
    summary: &MaterializedCephFsSource,
) -> CephFsSourceResult<MaterializedCephFsSource> {
    let data_source_id = &summary.data_source.id;
    let rel_path = crate::source_db::canonical_source_db_rel_path(data_source_id);
    let publication = CatalogPublicationRepo::new(request.case_conn).prepare(
        data_source_id,
        &attempt.attempt_id,
        &attempt.input_fingerprint,
        &rel_path,
        &summary.catalog_digest,
    )?;
    crate::source_db::publish_source_build_db(
        request.case_root,
        data_source_id,
        &attempt.attempt_id,
    )?;
    CatalogPublicationRepo::new(request.case_conn).mark_published(
        data_source_id,
        &attempt.attempt_id,
        &publication.seal,
    )?;
    let transaction = request
        .case_conn
        .unchecked_transaction()
        .map_err(persistence_sqlite::DbError::from)?;
    complete_catalog(&transaction, data_source_id, attempt, summary)?;
    DataSourceRepo::new(&transaction).update_import_state(data_source_id, "ready", None)?;
    transaction
        .commit()
        .map_err(persistence_sqlite::DbError::from)?;
    Ok(summary.clone())
}

fn preserve_incomplete(
    request: &CephFsSourceMaterializationRequest<'_>,
    attempt: &super::catalog::CatalogAttempt,
    summary: &MaterializedCephFsSource,
) -> CephFsSourceResult<MaterializedCephFsSource> {
    crate::source_db::preserve_unpublished_source_build_db(
        request.case_root,
        &summary.data_source.id,
        &attempt.attempt_id,
    )?;
    let transaction = request
        .case_conn
        .unchecked_transaction()
        .map_err(persistence_sqlite::DbError::from)?;
    defer_incomplete_catalog(&transaction, &summary.data_source.id, attempt, summary)?;
    DataSourceRepo::new(&transaction).update_import_state(
        &summary.data_source.id,
        "failed",
        Some("CephFS namespace closure could not be proven; diagnostics retained"),
    )?;
    transaction
        .commit()
        .map_err(persistence_sqlite::DbError::from)?;
    Ok(summary.clone())
}

pub(super) fn load_ready_summary(
    case_conn: &rusqlite::Connection,
    case_root: &std::path::Path,
    source: &domain::DataSource,
    lineage_fingerprint: &str,
) -> CephFsSourceResult<MaterializedCephFsSource> {
    let lineage =
        persistence_sqlite::repositories::ceph_fs_lineage_repo::CephFsDerivedLineageRepo::new(
            case_conn,
        )
        .find_by_data_source(&source.id.0)?
        .ok_or_else(|| {
            CephFsSourceError::InconsistentState("CephFS lineage is missing".to_string())
        })?;
    let db_path = crate::source_db::registered_source_db_path(case_conn, case_root, &source.id)?;
    crate::source_db::verify_finalized_source_db(&db_path, &source.id)?;
    let connection =
        crate::source_db::open_registered_source_db_read_only(case_conn, case_root, &source.id)?;
    let published =
        persistence_sqlite::repositories::ceph_fs_namespace_repo::CephFsNamespaceRepo::new(
            &connection,
        )
        .verify_published_catalog(
            &lineage.lineage.filesystem_identity,
            &source.id.0,
            &source.name,
        )?;
    let manifest = published.manifest;
    if manifest.input_sha256 != lineage.lineage.namespace_input_sha256
        || manifest.projection_sha256 != lineage.lineage.namespace_projection_sha256
        || manifest.filesystem_id != lineage.lineage.filesystem_id
        || manifest.fsmap_epoch != lineage.lineage.fsmap_epoch
    {
        return Err(CephFsSourceError::StalePublication);
    }
    validate_assembly_record(&connection, &lineage.lineage, &source.id.0, true)?;
    let capability = read_capability_record(&connection, &lineage.lineage, &source.id.0)?;
    let fingerprint = catalog_input_fingerprint(lineage_fingerprint);
    let rel_path = crate::source_db::canonical_source_db_rel_path(&source.id);
    if !manifest.published
        || !CatalogPublicationRepo::new(case_conn).is_published(
            &source.id,
            &fingerprint,
            &rel_path,
            &lineage.lineage.namespace_projection_sha256,
        )?
    {
        return Err(CephFsSourceError::StalePublication);
    }
    Ok(MaterializedCephFsSource {
        data_source: source.clone(),
        file_count: published.summary.file_count,
        directory_count: published.summary.directory_count,
        total_size: published.summary.total_size,
        catalog_digest: manifest.projection_sha256,
        capability,
        published: true,
    })
}

pub(super) fn validate_assembly_record(
    connection: &rusqlite::Connection,
    lineage: &persistence_sqlite::repositories::ceph_fs_lineage_repo::CephFsDerivedLineageRecord,
    data_source_id: &str,
    require_complete: bool,
) -> CephFsSourceResult<()> {
    let record = persistence_sqlite::repositories::ceph_fs_namespace_assembly_repo::
        CephFsNamespaceAssemblyRepo::new(connection)
        .find(&lineage.filesystem_identity, data_source_id)?
        .ok_or_else(|| {
            CephFsSourceError::InconsistentState(
                "CephFS namespace assembly record is missing".to_string(),
            )
        })?;
    if record.assembly_sha256 != lineage.namespace_assembly_sha256
        || (require_complete && (!record.complete || record.frozen))
    {
        return Err(CephFsSourceError::StalePublication);
    }
    Ok(())
}

pub(super) fn read_capability_record(
    connection: &rusqlite::Connection,
    lineage: &persistence_sqlite::repositories::ceph_fs_lineage_repo::CephFsDerivedLineageRecord,
    data_source_id: &str,
) -> CephFsSourceResult<super::CephFsSourceCapability> {
    let record =
        persistence_sqlite::repositories::ceph_fs_capability_repo::CephFsSourceCapabilityRepo::new(
            connection,
        )
        .find(&lineage.filesystem_identity, data_source_id)?
        .ok_or_else(|| {
            CephFsSourceError::InconsistentState(
                "CephFS source capability record is missing".to_string(),
            )
        })?;
    if record.lineage_fingerprint != lineage.lineage_fingerprint
        || record.assembly_sha256 != lineage.namespace_assembly_sha256
        || record.namespace_projection_sha256 != lineage.namespace_projection_sha256
        || record.capability.as_str() != lineage.source_capability
    {
        return Err(CephFsSourceError::StalePublication);
    }
    Ok(match record.capability {
        persistence_sqlite::repositories::ceph_fs_capability_repo::CephFsSourceCapability::
            MetadataOnly => super::CephFsSourceCapability::MetadataOnly,
        persistence_sqlite::repositories::ceph_fs_capability_repo::CephFsSourceCapability::
            MetadataBrowseable => super::CephFsSourceCapability::MetadataBrowseable,
        persistence_sqlite::repositories::ceph_fs_capability_repo::CephFsSourceCapability::
            BoundedPreview => super::CephFsSourceCapability::BoundedPreview,
    })
}

fn validate_cluster(request: &CephFsSourceMaterializationRequest<'_>) -> CephFsSourceResult<()> {
    let cluster = DataSourceClusterRepo::new(request.case_conn)
        .find_by_id(request.cluster_id)?
        .ok_or(CephFsSourceError::InvalidInput("parent cluster is missing"))?;
    if cluster.case_id != *request.case_id || cluster.import_state != "ready" {
        return Err(CephFsSourceError::InvalidInput(
            "parent cluster does not belong to the case or is not ready",
        ));
    }
    Ok(())
}
