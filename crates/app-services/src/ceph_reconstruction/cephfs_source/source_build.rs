use std::path::Path;

use domain::{CaseId, DataSource, DataSourceId, EntryType, FileEntry};
use persistence_source::repositories::{
    ceph_fs_namespace_repo::{CephFsNamespaceProjection, CephFsNamespaceRepo},
    file_repo::FileRepo,
};
use persistence_sqlite::repositories::{
    ceph_fs_capability_repo::{
        CephFsSourceCapability as PersistedCapability, CephFsSourceCapabilityRecord,
        CephFsSourceCapabilityRepo,
    },
    ceph_fs_namespace_assembly_repo::{CephFsNamespaceAssemblyRecord, CephFsNamespaceAssemblyRepo},
    datasource_repo::DataSourceRepo,
};

use super::{
    CephFsSourceCapability, CephFsSourceError, CephFsSourceResult, MaterializedCephFsSource,
};

pub(super) struct CephFsSourceBuildRequest<'a> {
    pub case_root: &'a Path,
    pub case_id: &'a CaseId,
    pub source: &'a DataSource,
    pub attempt_id: &'a str,
    pub projection: &'a CephFsNamespaceProjection,
    pub file_entries: &'a [FileEntry],
    pub assembly: &'a ceph_wire::CephFsNamespaceAssembly,
    pub capability: CephFsSourceCapability,
    pub lineage_fingerprint: &'a str,
}

pub(super) fn build_source_database(
    request: CephFsSourceBuildRequest<'_>,
) -> CephFsSourceResult<MaterializedCephFsSource> {
    let connection = crate::source_db::open_fresh_source_build_db(
        request.case_root,
        &request.source.id,
        request.attempt_id,
    )?;
    let build = (|| {
        write_projection_metadata(&connection, &request)?;
        write_catalog_generation(&connection, &request)?;
        crate::source_db::finalize_source_build_db(&connection)?;
        Ok(summary(
            request.source,
            request.projection,
            request.file_entries,
            request.capability,
        ))
    })();
    drop(connection);
    build
}

fn write_projection_metadata(
    connection: &rusqlite::Connection,
    request: &CephFsSourceBuildRequest<'_>,
) -> CephFsSourceResult<()> {
    DataSourceRepo::new(connection)
        .upsert_source_local_metadata(request.case_id, request.source)?;
    CephFsNamespaceRepo::new(connection).replace(request.projection)?;
    CephFsNamespaceAssemblyRepo::new(connection).replace(&assembly_record(
        request.source,
        &request.projection.manifest.filesystem_identity,
        request.assembly,
    )?)?;
    CephFsSourceCapabilityRepo::new(connection).replace(&capability_record(
        request.source,
        request.capability,
        request.lineage_fingerprint,
        &request.projection.manifest.projection_sha256,
        &request.projection.manifest.filesystem_identity,
        request.assembly,
    ))?;
    Ok(())
}

fn write_catalog_generation(
    connection: &rusqlite::Connection,
    request: &CephFsSourceBuildRequest<'_>,
) -> CephFsSourceResult<()> {
    if request.projection.manifest.published {
        FileRepo::new(connection).insert_batch(request.file_entries)?;
        verify_file_catalog(
            connection,
            request.projection,
            &request.source.id,
            &request.source.name,
            request.file_entries,
        )?;
        publish_native_catalog(connection, request)?;
    } else if !request.file_entries.is_empty() {
        return Err(CephFsSourceError::InconsistentState(
            "incomplete CephFS namespace wrote file tree rows".to_string(),
        ));
    } else {
        remove_incomplete_source_build(request)?;
    }
    Ok(())
}

fn publish_native_catalog(
    connection: &rusqlite::Connection,
    request: &CephFsSourceBuildRequest<'_>,
) -> CephFsSourceResult<()> {
    let store = persistence_source::detail::open_store_for_write(connection)?.ok_or_else(|| {
        CephFsSourceError::InconsistentState("CephFS native source store is missing".to_string())
    })?;
    let projection_digest = hex::encode(persistence_source::fjall::digest(
        request.projection.manifest.projection_sha256.as_bytes(),
    ));
    persistence_source::fjall::seal_source_generation(
        &store,
        &request.source.id.0,
        request.attempt_id,
        &projection_digest,
    )
    .map_err(|error| CephFsSourceError::InconsistentState(error.to_string()))?;
    let key = persistence_source::fjall::encode_key(
        "projection",
        &[persistence_source::fjall::KeyPart::Text(
            request.source.id.0.clone(),
        )],
    )
    .map_err(|error| CephFsSourceError::InconsistentState(error.to_string()))?;
    let marker = persistence_source::fjall::VersionedEnvelope::new(
        "source_projection_complete",
        1,
        format!("projection:{}", request.source.id.0),
        &request.source.id.0,
        1,
        request.source.id.0.as_bytes().to_vec(),
    );
    store
        .write_envelope(
            persistence_source::fjall::KeyspaceName::Meta,
            key,
            &marker,
            persistence_source::fjall::PersistMode::SyncAll,
        )
        .map_err(|error| CephFsSourceError::InconsistentState(error.to_string()))?;
    Ok(())
}

fn remove_incomplete_source_build(
    request: &CephFsSourceBuildRequest<'_>,
) -> CephFsSourceResult<()> {
    let layout = persistence_source::fjall::SourceBuildLayout::for_case_source(
        request.case_root,
        &request.source.id.0,
        request.attempt_id,
    )
    .map_err(|error| CephFsSourceError::InconsistentState(error.to_string()))?;
    if layout.build_path().is_dir() {
        let _close_guard = crate::storage_runtime::close_source_store_at_path(&layout.build_path())
            .map_err(|error| CephFsSourceError::InconsistentState(error.to_string()))?
            .ok_or_else(|| {
                CephFsSourceError::InconsistentState(
                    "incomplete CephFS source store is still in use".to_string(),
                )
            })?;
        std::fs::remove_dir_all(layout.build_path())?;
    }
    Ok(())
}

fn verify_file_catalog(
    connection: &rusqlite::Connection,
    projection: &CephFsNamespaceProjection,
    data_source_id: &DataSourceId,
    source_name: &str,
    expected: &[FileEntry],
) -> CephFsSourceResult<()> {
    let verified = CephFsNamespaceRepo::new(connection).verify_published_catalog(
        &projection.manifest.filesystem_identity,
        &data_source_id.0,
        source_name,
    )?;
    if verified.summary.file_count != expected.len() as u64 {
        return Err(CephFsSourceError::InconsistentState(
            "CephFS file catalog has an unexpected row count".to_string(),
        ));
    }
    Ok(())
}

fn summary(
    source: &DataSource,
    projection: &CephFsNamespaceProjection,
    entries: &[FileEntry],
    capability: CephFsSourceCapability,
) -> MaterializedCephFsSource {
    MaterializedCephFsSource {
        data_source: source.clone(),
        file_count: entries.len() as u64,
        directory_count: entries
            .iter()
            .filter(|entry| entry.entry_type == EntryType::Directory)
            .count() as u64,
        total_size: entries
            .iter()
            .filter(|entry| entry.entry_type == EntryType::File)
            .filter_map(|entry| entry.size)
            .fold(0u64, u64::saturating_add),
        catalog_digest: projection.manifest.projection_sha256.clone(),
        capability,
        published: projection.manifest.published,
    }
}

fn assembly_record(
    source: &DataSource,
    filesystem_identity: &str,
    assembly: &ceph_wire::CephFsNamespaceAssembly,
) -> CephFsSourceResult<CephFsNamespaceAssemblyRecord> {
    let reasons = assembly
        .freeze_reasons()
        .iter()
        .map(|reason| reason.code())
        .collect::<Vec<_>>();
    let freeze_reasons_json = serde_json::to_string(&reasons).map_err(|_| {
        CephFsSourceError::InvalidInput("namespace freeze reasons cannot be serialized")
    })?;
    Ok(CephFsNamespaceAssemblyRecord {
        filesystem_identity: filesystem_identity.to_string(),
        data_source_id: source.id.0.clone(),
        assembly_sha256: assembly.assembly_sha256().to_string(),
        assembly_version: ceph_wire::CEPHFS_NAMESPACE_ASSEMBLY_VERSION,
        complete: assembly.is_complete(),
        frozen: assembly.is_frozen(),
        freeze_reasons_json,
        mutation_state: assembly.mutation_state().code().to_string(),
        mutation_digest: assembly.mutation_state().digest().map(str::to_string),
    })
}

fn capability_record(
    source: &DataSource,
    capability: CephFsSourceCapability,
    lineage_fingerprint: &str,
    projection_sha256: &str,
    filesystem_identity: &str,
    assembly: &ceph_wire::CephFsNamespaceAssembly,
) -> CephFsSourceCapabilityRecord {
    CephFsSourceCapabilityRecord {
        filesystem_identity: filesystem_identity.to_string(),
        data_source_id: source.id.0.clone(),
        capability: match capability {
            CephFsSourceCapability::MetadataOnly => PersistedCapability::MetadataOnly,
            CephFsSourceCapability::MetadataBrowseable => PersistedCapability::MetadataBrowseable,
            CephFsSourceCapability::BoundedPreview => PersistedCapability::BoundedPreview,
        },
        lineage_fingerprint: lineage_fingerprint.to_string(),
        assembly_sha256: assembly.assembly_sha256().to_string(),
        namespace_projection_sha256: projection_sha256.to_string(),
        schema_version: 1,
        decoder_profile: "cephfs-namespace-v1".to_string(),
    }
}
