use persistence_sqlite::repositories::{
    catalog_publication_repo::CatalogPublicationRepo,
    ceph_fs_lineage_repo::CephFsDerivedLineageAggregate,
    datasource_repo::DataSourceRepo,
    linux_topology_artifact_repo::{LinuxTopologyArtifactRecord, LinuxTopologyArtifactRepo},
    storage_object_repo::{StorageObjectRecord, StorageObjectRepo},
};
use rusqlite::OptionalExtension;

use super::{
    capability::derive_source_capability,
    catalog::{
        catalog_input_fingerprint, claim_catalog, defer_incomplete_catalog, fail_catalog_message,
        refresh_catalog, CatalogClaim,
    },
    lineage::{
        build_data_source, build_lineage, derived_source_id, source_storage, CephFsLineageEvidence,
    },
    presence_gate::validate_presence,
    projection::{build_file_entries, build_namespace_projection},
    publication::{
        map_error, publication_request, CephFsPublicationAttempt, CephFsPublicationValidator,
    },
    recovery::recover_published_source,
    registration::{bind_derived_revision, ensure_registration, import_state},
    source_build::{build_source_database, CephFsSourceBuildRequest},
    CephFsSourceError, CephFsSourceMaterializationRequest, CephFsSourceResult,
    MaterializedCephFsSource,
};

pub fn materialize_cephfs_source(
    request: CephFsSourceMaterializationRequest<'_>,
) -> CephFsSourceResult<MaterializedCephFsSource> {
    persistence_source::publication::catalog_publication::ensure_not_cancelled(request.cancel)
        .map_err(map_error)?;
    validate_ceph_scope(&request)?;
    validate_presence(request.presence, request.descriptor)?;
    let namespace = ceph_wire::assemble_cephfs_namespace(request.namespace_assembly_input.clone())?;
    persistence_source::publication::catalog_publication::ensure_not_cancelled(request.cancel)
        .map_err(map_error)?;
    let data_source_id = derived_source_id(&request.ceph_scope_id.0, &request.descriptor.identity)?;
    let projection = build_namespace_projection(
        &data_source_id,
        request.descriptor,
        &namespace,
        request.namespace_input_sha256,
        request.inline_data_by_inode,
        request.sparse_extents_by_inode,
    )?;
    let desired_source = build_data_source(
        &request.ceph_scope_id.0,
        &data_source_id,
        request.descriptor,
    );
    let storage = source_storage(&data_source_id);
    let capability = derive_source_capability(&namespace, &projection);
    let lineage = build_lineage(
        &data_source_id,
        &request.ceph_scope_id.0,
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
    register_cephfs_storage_object(&request, &lineage)?;
    let source = ensure_registration(
        request.case_conn,
        request.case_id,
        &desired_source,
        &storage,
        &lineage,
    )?;
    register_cephfs_artifact(&request, &source)?;
    if import_state(request.case_conn, &data_source_id)? == "ready" {
        return load_ready_summary(request.case_conn, request.case_root, &source);
    }
    if source_kernel::source_db::source_db_path(request.case_root, &data_source_id).exists()
        || CatalogPublicationRepo::new(request.case_conn)
            .find(&data_source_id)?
            .is_some()
    {
        if let Some(summary) = recover_published_source(
            request.case_conn,
            request.case_root,
            &source,
            &lineage,
            request.cancel,
        )? {
            return Ok(summary);
        }
        return Err(CephFsSourceError::RetainedIncompleteSource);
    }
    let catalog_fingerprint = catalog_input_fingerprint(&lineage.lineage.lineage_fingerprint);
    let attempt = match claim_catalog(request.case_conn, &data_source_id, &catalog_fingerprint)? {
        CatalogClaim::Acquired(attempt) => attempt,
        CatalogClaim::Ready => {
            return load_ready_summary(request.case_conn, request.case_root, &source)
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
            lineage: &lineage,
            lineage_fingerprint: &lineage.lineage.lineage_fingerprint,
            capability,
        },
    );
    if let Err(error) = &result {
        record_catalog_failure(&request, &data_source_id, &attempt, error)?;
    }
    result
}

fn register_cephfs_storage_object(
    request: &CephFsSourceMaterializationRequest<'_>,
    lineage: &persistence_sqlite::repositories::ceph_fs_lineage_repo::CephFsDerivedLineageAggregate,
) -> CephFsSourceResult<()> {
    StorageObjectRepo::new(request.case_conn).insert_if_absent(&StorageObjectRecord {
        id: lineage.lineage.parent_storage_object_id.clone(),
        case_id: request.case_id.0.clone(),
        object_kind: "ceph_fs".to_string(),
        name: request.descriptor.name.clone(),
        identity_state: "candidate".to_string(),
        status: "ready".to_string(),
        provenance_json: serde_json::json!({"cephScopeId": request.ceph_scope_id.0}).to_string(),
    })?;
    Ok(())
}

fn register_cephfs_artifact(
    request: &CephFsSourceMaterializationRequest<'_>,
    source: &domain::DataSource,
) -> CephFsSourceResult<()> {
    LinuxTopologyArtifactRepo::new(request.case_conn).insert(&LinuxTopologyArtifactRecord {
        scope_id: request.ceph_scope_id.0.clone(),
        data_source_id: source.id.0.clone(),
        file_id: Some(request.descriptor.identity.clone()),
        layer: "storage".to_string(),
        artifact_kind: "ceph_fs".to_string(),
        parser: "cephfs-namespace-v1".to_string(),
        status: "candidate_found".to_string(),
        diagnostics_json: "[]".to_string(),
        content_digest: Some(request.namespace_input_sha256.to_string()),
    })?;
    Ok(())
}

struct CatalogRun<'a> {
    data_source_id: &'a domain::DataSourceId,
    source: &'a domain::DataSource,
    attempt: &'a super::catalog::CatalogAttempt,
    projection:
        &'a persistence_source::repositories::ceph_fs_namespace_repo::CephFsNamespaceProjection,
    namespace: &'a ceph_wire::CephFsNamespaceAssembly,
    lineage: &'a CephFsDerivedLineageAggregate,
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
        lineage,
        lineage_fingerprint,
        capability,
    } = run;
    let file_entries = if projection.manifest.published {
        build_file_entries(data_source_id, &source.name, namespace.graph())?
    } else {
        Vec::new()
    };
    let (result, publication_digest) = build_source_database(CephFsSourceBuildRequest {
        cancel: request.cancel,
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
    persistence_source::publication::catalog_publication::ensure_not_cancelled(request.cancel)
        .map_err(map_error)?;
    bind_derived_revision(
        request.case_conn,
        request.case_id,
        data_source_id,
        result.total_size,
        result.catalog_digest.clone(),
    )?;
    refresh_catalog(request.case_conn, data_source_id, attempt)?;
    if result.published {
        let publication_digest = publication_digest.ok_or_else(|| {
            CephFsSourceError::InconsistentState(
                "published CephFS generation has no receipt".to_string(),
            )
        })?;
        publish_complete(request, attempt, &result, &publication_digest, lineage)
    } else {
        preserve_incomplete(request, attempt, &result)
    }
}

fn record_catalog_failure(
    request: &CephFsSourceMaterializationRequest<'_>,
    data_source_id: &domain::DataSourceId,
    attempt: &super::catalog::CatalogAttempt,
    error: &CephFsSourceError,
) -> CephFsSourceResult<()> {
    let cancelled = matches!(error, CephFsSourceError::Cancelled);
    let transaction = request
        .case_conn
        .unchecked_transaction()
        .map_err(persistence_sqlite::DbError::from)?;
    fail_catalog_message(
        &transaction,
        data_source_id,
        attempt,
        &error.to_string(),
        cancelled,
    )?;
    DataSourceRepo::new(&transaction).update_import_state(
        data_source_id,
        if cancelled { "pending" } else { "failed" },
        Some(&error.to_string()),
    )?;
    transaction
        .commit()
        .map_err(persistence_sqlite::DbError::from)?;
    persistence_source::publication::catalog_publication::discard_unpublished_catalog_build(
        request.case_conn,
        request.case_root,
        data_source_id,
        &attempt.attempt_id,
    )
    .map_err(map_error)?;
    Ok(())
}

fn publish_complete(
    request: &CephFsSourceMaterializationRequest<'_>,
    attempt: &super::catalog::CatalogAttempt,
    summary: &MaterializedCephFsSource,
    publication_digest: &str,
    lineage: &CephFsDerivedLineageAggregate,
) -> CephFsSourceResult<MaterializedCephFsSource> {
    let validator = CephFsPublicationValidator {
        case_root: request.case_root,
        lineage,
    };
    let publication_attempt = CephFsPublicationAttempt { attempt, summary };
    let fingerprint = catalog_input_fingerprint(&lineage.lineage.lineage_fingerprint);
    let publication = publication_request(
        request.case_conn,
        request.case_root,
        request.case_id,
        &summary.data_source,
        &fingerprint,
        &publication_attempt,
        &validator,
        request.cancel,
    );
    persistence_source::publication::catalog_publication::publish_build(
        &publication,
        publication_digest,
    )
    .map_err(map_error)?;
    Ok(summary.clone())
}

fn preserve_incomplete(
    request: &CephFsSourceMaterializationRequest<'_>,
    attempt: &super::catalog::CatalogAttempt,
    summary: &MaterializedCephFsSource,
) -> CephFsSourceResult<MaterializedCephFsSource> {
    source_kernel::source_db::preserve_unpublished_source_build_db(
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
) -> CephFsSourceResult<MaterializedCephFsSource> {
    let lineage =
        persistence_sqlite::repositories::ceph_fs_lineage_repo::CephFsDerivedLineageRepo::new(
            case_conn,
        )
        .find_by_data_source(&source.id.0)?
        .ok_or_else(|| {
            CephFsSourceError::InconsistentState("CephFS lineage is missing".to_string())
        })?;
    let summary = load_published_summary(case_conn, case_root, source, &lineage)?;
    let fingerprint = catalog_input_fingerprint(&lineage.lineage.lineage_fingerprint);
    let validator = CephFsPublicationValidator {
        case_root,
        lineage: &lineage,
    };
    persistence_source::publication::catalog_publication::verify_publication(
        case_conn,
        case_root,
        source,
        &fingerprint,
        &DataSourceRepo::new(case_conn).case_id(&source.id)?,
        persistence_source::publication::catalog_publication::CatalogProjection::SourceOnly,
        &validator,
    )
    .map_err(map_error)?;
    Ok(summary)
}

pub(super) fn load_published_summary(
    case_conn: &rusqlite::Connection,
    case_root: &std::path::Path,
    source: &domain::DataSource,
    lineage: &CephFsDerivedLineageAggregate,
) -> CephFsSourceResult<MaterializedCephFsSource> {
    let db_path =
        source_kernel::source_db::registered_source_db_path(case_conn, case_root, &source.id)?;
    source_kernel::source_db::verify_finalized_source_db(&db_path, &source.id)?;
    let connection = source_kernel::source_db::open_registered_source_db_read_only(
        case_conn, case_root, &source.id,
    )?;
    let published =
        persistence_source::repositories::ceph_fs_namespace_repo::CephFsNamespaceRepo::new(
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
    if !manifest.published {
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

fn validate_ceph_scope(request: &CephFsSourceMaterializationRequest<'_>) -> CephFsSourceResult<()> {
    let scope: Option<(String, String)> = request
        .case_conn
        .query_row(
            "SELECT case_id, status FROM linux_topology_scopes
             WHERE id = ?1 AND scope_kind = 'ceph'",
            [&request.ceph_scope_id.0],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(persistence_sqlite::DbError::from)?;
    if !matches!(scope, Some((ref case_id, ref status)) if case_id == &request.case_id.0 && status == "ready")
    {
        return Err(CephFsSourceError::InvalidInput(
            "parent Ceph scope does not belong to the case or is not ready",
        ));
    }
    Ok(())
}
