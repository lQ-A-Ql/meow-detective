use std::path::Path;

use domain::{CaseId, DataSource, DataSourceId, EntryType, FileEntry};
use persistence_sqlite::repositories::{
    ceph_fs_capability_repo::{
        CephFsSourceCapability as PersistedCapability, CephFsSourceCapabilityRecord,
        CephFsSourceCapabilityRepo,
    },
    ceph_fs_namespace_assembly_repo::{CephFsNamespaceAssemblyRecord, CephFsNamespaceAssemblyRepo},
    ceph_fs_namespace_repo::{CephFsNamespaceProjection, CephFsNamespaceRepo},
    datasource_repo::DataSourceRepo,
    file_repo::FileRepo,
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
    let CephFsSourceBuildRequest {
        case_root,
        case_id,
        source,
        attempt_id,
        projection,
        file_entries,
        assembly,
        capability,
        lineage_fingerprint,
    } = request;
    let connection =
        crate::source_db::open_fresh_source_build_db(case_root, &source.id, attempt_id)?;
    let build = (|| {
        DataSourceRepo::new(&connection).upsert_source_local_metadata(case_id, source)?;
        CephFsNamespaceRepo::new(&connection).replace(projection)?;
        CephFsNamespaceAssemblyRepo::new(&connection).replace(&assembly_record(
            source,
            &projection.manifest.filesystem_identity,
            assembly,
        )?)?;
        CephFsSourceCapabilityRepo::new(&connection).replace(&capability_record(
            source,
            capability,
            lineage_fingerprint,
            &projection.manifest.projection_sha256,
            &projection.manifest.filesystem_identity,
            assembly,
        ))?;
        if projection.manifest.published {
            FileRepo::new(&connection).insert_batch(file_entries)?;
            verify_file_catalog(
                &connection,
                projection,
                &source.id,
                &source.name,
                file_entries,
            )?;
        } else if FileRepo::new(&connection).count_all()? != 0 {
            return Err(CephFsSourceError::InconsistentState(
                "incomplete CephFS namespace wrote file tree rows".to_string(),
            ));
        }
        crate::source_db::finalize_source_build_db(&connection)?;
        Ok(summary(source, projection, file_entries, capability))
    })();
    drop(connection);
    build
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
