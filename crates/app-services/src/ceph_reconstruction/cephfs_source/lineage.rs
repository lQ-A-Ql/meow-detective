use std::{path::PathBuf, str::FromStr};

use chrono::{DateTime, Utc};
use domain::{
    CaseId, DataSource, DataSourceHashStatus, DataSourceId, DataSourceKind, DataSourcePlatform,
    DataSourceProvenance, DataSourceProvenanceStatus,
};
use persistence_sqlite::repositories::{
    catalog_publication_repo::CatalogPublicationRepo,
    ceph_fs_lineage_repo::{
        cephfs_lineage_fingerprint, CephFsDerivedLineageAggregate, CephFsDerivedLineageRecord,
        CephFsDerivedLineageRepo, CephFsDerivedMapProvenanceRecord, CephFsDerivedPoolRecord,
        CephFsDerivedPoolSourceRecord,
    },
    ceph_fs_namespace_repo::CephFsNamespaceRepo,
    datasource_repo::{DataSourceRepo, DataSourceStorage},
};
use sha2::{Digest, Sha256};

use super::{CephFsSourceError, CephFsSourceResult};
use crate::ceph_reconstruction::{
    CephFsDescriptor, CephFsDescriptorState, CephFsMapProvenance, CephFsObjectSource,
    CephFsPoolBinding, CephFsPoolProvenance, CephFsPoolRole,
};

pub(super) fn derived_source_id(
    cluster_id: &str,
    filesystem_identity: &str,
) -> CephFsSourceResult<DataSourceId> {
    validate_text(cluster_id)?;
    validate_text(filesystem_identity)?;
    let mut hasher = Sha256::new();
    hasher.update(b"meow-detective-cephfs-source-id-v1");
    for value in [cluster_id.as_bytes(), filesystem_identity.as_bytes()] {
        hasher.update((value.len() as u64).to_le_bytes());
        hasher.update(value);
    }
    let digest = hex::encode(hasher.finalize());
    Ok(DataSourceId(format!("cephfs-{}", &digest[..32])))
}

pub(super) fn build_data_source(
    cluster_id: &str,
    data_source_id: &DataSourceId,
    descriptor: &CephFsDescriptor,
) -> DataSource {
    DataSource {
        id: data_source_id.clone(),
        name: format!("CephFS {}", descriptor.name),
        kind: DataSourceKind::CephFs,
        source_path: PathBuf::from(format!(
            "cephfs://{cluster_id}/{}",
            descriptor.filesystem_id
        )),
        imported_at: Utc::now(),
        provenance: DataSourceProvenance {
            source_hash_sha256: None,
            hash_status: DataSourceHashStatus::Unavailable,
            canonical_source_path: None,
            evidence_size: None,
            reader_kind: Some("ceph-fs".to_string()),
            provenance_status: DataSourceProvenanceStatus::Recorded,
            warnings: Vec::new(),
        },
    }
}

pub(super) fn source_storage(data_source_id: &DataSourceId) -> DataSourceStorage {
    DataSourceStorage::source_db(
        &data_source_id.0,
        Some(DataSourcePlatform::Linux.as_storage_str()),
        Some("ceph_fs".to_string()),
    )
}

pub(super) struct CephFsLineageEvidence<'a> {
    pub namespace_input_sha256: &'a str,
    pub namespace_projection_sha256: &'a str,
    pub namespace_assembly_sha256: &'a str,
    pub source_capability: super::CephFsSourceCapability,
    pub journal_boundary_sha256: Option<&'a str>,
    pub expected_replica_count: usize,
}

pub(super) fn build_lineage(
    data_source_id: &DataSourceId,
    cluster_id: &str,
    descriptor: &CephFsDescriptor,
    evidence: CephFsLineageEvidence<'_>,
) -> CephFsSourceResult<CephFsDerivedLineageAggregate> {
    let expected_replica_count = u32::try_from(evidence.expected_replica_count)
        .map_err(|_| CephFsSourceError::InvalidInput("replica count overflows"))?;
    if descriptor.state != CephFsDescriptorState::Present || expected_replica_count == 0 {
        return Err(CephFsSourceError::InvalidInput(
            "descriptor is not present or replica count is zero",
        ));
    }
    let mut pools = Vec::with_capacity(descriptor.data_pools.len().saturating_add(1));
    pools.push(pool_record(&descriptor.metadata_pool, "metadata", 0)?);
    for pool in &descriptor.data_pools {
        let CephFsPoolRole::Data { ordinal } = pool.role else {
            return Err(CephFsSourceError::InvalidInput(
                "data pool has a non-data role",
            ));
        };
        pools.push(pool_record(pool, "data", ordinal)?);
    }
    pools.sort_by_key(|pool| (pool.role != "metadata", pool.ordinal));
    let mut map_provenance = descriptor
        .provenance
        .iter()
        .map(|item| CephFsDerivedMapProvenanceRecord {
            ordinal: 0,
            source_data_source_id: item.source_identity.clone(),
            inventory_id: item.inventory_identity.clone(),
            captured_at: item.captured_at.to_rfc3339(),
            raw_fsmap_sha256: item.raw_fsmap_sha256.clone(),
            raw_mdsmap_sha256: item.raw_mdsmap_sha256.clone(),
        })
        .collect::<Vec<_>>();
    map_provenance.sort_by(|left, right| {
        (&left.source_data_source_id, &left.inventory_id)
            .cmp(&(&right.source_data_source_id, &right.inventory_id))
    });
    for (ordinal, item) in map_provenance.iter_mut().enumerate() {
        item.ordinal = u32::try_from(ordinal)
            .map_err(|_| CephFsSourceError::InvalidInput("provenance count overflows"))?;
    }
    let mut aggregate = CephFsDerivedLineageAggregate {
        lineage: CephFsDerivedLineageRecord {
            derived_data_source_id: data_source_id.0.clone(),
            parent_cluster_id: cluster_id.to_string(),
            cluster_identity: descriptor.cluster_identity.clone(),
            filesystem_identity: descriptor.identity.clone(),
            filesystem_id: descriptor.filesystem_id,
            filesystem_name: descriptor.name.clone(),
            fsmap_epoch: descriptor.fsmap_epoch,
            mdsmap_epoch: descriptor.mdsmap_epoch,
            descriptor_state: "present".to_string(),
            metadata_pool_id: descriptor.metadata_pool.pool_id,
            expected_replica_count,
            namespace_input_sha256: evidence.namespace_input_sha256.to_string(),
            namespace_projection_sha256: evidence.namespace_projection_sha256.to_string(),
            namespace_assembly_sha256: evidence.namespace_assembly_sha256.to_string(),
            source_capability: evidence.source_capability.as_str().to_string(),
            namespace_schema_version: persistence_sqlite::repositories::ceph_fs_namespace_repo::CEPHFS_NAMESPACE_SCHEMA_VERSION,
            decoder_profile: persistence_sqlite::repositories::ceph_fs_namespace_repo::CEPHFS_NAMESPACE_DECODER_PROFILE.to_string(),
            journal_boundary_sha256: evidence.journal_boundary_sha256.map(str::to_string),
            lineage_fingerprint: String::new(),
        },
        pools,
        map_provenance,
    };
    aggregate.lineage.lineage_fingerprint = cephfs_lineage_fingerprint(&aggregate);
    Ok(aggregate)
}

fn pool_record(
    pool: &CephFsPoolBinding,
    role: &str,
    ordinal: u32,
) -> CephFsSourceResult<CephFsDerivedPoolRecord> {
    let mut sources = pool
        .provenance
        .iter()
        .map(|source| CephFsDerivedPoolSourceRecord {
            ordinal: 0,
            source_data_source_id: source.source_identity.clone(),
            inventory_id: source.inventory_identity.clone(),
        })
        .collect::<Vec<_>>();
    sources.sort_by(|left, right| {
        (&left.source_data_source_id, &left.inventory_id)
            .cmp(&(&right.source_data_source_id, &right.inventory_id))
    });
    for (source_ordinal, source) in sources.iter_mut().enumerate() {
        source.ordinal = u32::try_from(source_ordinal)
            .map_err(|_| CephFsSourceError::InvalidInput("pool source count overflows"))?;
    }
    Ok(CephFsDerivedPoolRecord {
        pool_id: pool.pool_id,
        role: role.to_string(),
        ordinal,
        sources,
    })
}

pub(super) struct LoadedCephFsRuntime {
    pub descriptor: CephFsDescriptor,
    pub sources: Vec<CephFsObjectSource>,
    pub expected_replica_count: usize,
    pub lineage_fingerprint: String,
    pub resolved_pool_id: i64,
}

pub(super) fn load_runtime(
    case_conn: &rusqlite::Connection,
    case_root: &std::path::Path,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
    requested_pool_id: i64,
) -> CephFsSourceResult<LoadedCephFsRuntime> {
    let source = DataSourceRepo::new(case_conn)
        .find_by_case(case_id)?
        .into_iter()
        .find(|source| source.id == *data_source_id);
    let Some(source) = source else {
        return Err(CephFsSourceError::InconsistentState(
            "derived source does not belong to the active case".to_string(),
        ));
    };
    if source.kind != DataSourceKind::CephFs {
        return Err(CephFsSourceError::InconsistentState(
            "preview source kind is not CephFS".to_string(),
        ));
    }
    let storage = DataSourceRepo::new(case_conn)
        .find_storage(data_source_id)?
        .ok_or_else(|| {
            CephFsSourceError::InconsistentState(
                "CephFS derived source has no storage metadata".to_string(),
            )
        })?;
    validate_derived_storage(data_source_id, &storage)?;
    let aggregate = CephFsDerivedLineageRepo::new(case_conn)
        .find_by_data_source(&data_source_id.0)?
        .ok_or_else(|| {
            CephFsSourceError::InconsistentState("CephFS lineage is missing".to_string())
        })?;
    let descriptor = descriptor_from_lineage(&aggregate)?;
    let db_path =
        crate::source_db::registered_source_db_path(case_conn, case_root, data_source_id)?;
    crate::source_db::verify_finalized_source_db(&db_path, data_source_id)?;
    let source_connection = persistence_sqlite::open_existing_source_read_only(&db_path)?;
    let manifest = CephFsNamespaceRepo::new(&source_connection)
        .verify_published_catalog(
            &aggregate.lineage.filesystem_identity,
            &data_source_id.0,
            &source.name,
        )?
        .manifest;
    if manifest.input_sha256 != aggregate.lineage.namespace_input_sha256
        || manifest.projection_sha256 != aggregate.lineage.namespace_projection_sha256
        || manifest.filesystem_id != aggregate.lineage.filesystem_id
        || manifest.fsmap_epoch != aggregate.lineage.fsmap_epoch
    {
        return Err(CephFsSourceError::StalePublication);
    }
    let catalog_fingerprint =
        super::catalog::catalog_input_fingerprint(&aggregate.lineage.lineage_fingerprint);
    if !CatalogPublicationRepo::new(case_conn).is_published(
        data_source_id,
        &catalog_fingerprint,
        &crate::source_db::canonical_source_db_rel_path(data_source_id),
        &manifest.projection_sha256,
    )? {
        return Err(CephFsSourceError::InconsistentState(
            "CephFS Catalog publication seal is missing or stale".to_string(),
        ));
    }
    let resolved_pool_id = if requested_pool_id == -1 {
        descriptor
            .data_pools
            .first()
            .map(|pool| pool.pool_id)
            .ok_or(CephFsSourceError::InvalidInput(
                "default data pool is missing",
            ))?
    } else {
        requested_pool_id
    };
    let pool = aggregate
        .pools
        .iter()
        .find(|pool| pool.pool_id == resolved_pool_id && pool.role == "data")
        .ok_or(CephFsSourceError::InvalidInput(
            "file layout references an unbound data pool",
        ))?;
    let sources = load_parent_object_sources(case_conn, case_root, case_id, &pool.sources)?;
    Ok(LoadedCephFsRuntime {
        descriptor,
        sources,
        expected_replica_count: aggregate.lineage.expected_replica_count as usize,
        lineage_fingerprint: aggregate.lineage.lineage_fingerprint,
        resolved_pool_id,
    })
}

fn validate_derived_storage(
    data_source_id: &DataSourceId,
    storage: &DataSourceStorage,
) -> CephFsSourceResult<()> {
    if storage.storage_model != "source_db"
        || !storage
            .platform
            .eq_ignore_ascii_case(DataSourcePlatform::Linux.as_storage_str())
        || storage.profile.as_deref() != Some("ceph_fs")
        || !storage.import_state.eq_ignore_ascii_case("ready")
    {
        return Err(CephFsSourceError::InconsistentState(format!(
            "CephFS source {} is not ready for preview",
            data_source_id.0
        )));
    }
    Ok(())
}

fn load_parent_object_sources(
    case_conn: &rusqlite::Connection,
    case_root: &std::path::Path,
    case_id: &CaseId,
    bindings: &[CephFsDerivedPoolSourceRecord],
) -> CephFsSourceResult<Vec<CephFsObjectSource>> {
    let case_sources = DataSourceRepo::new(case_conn).find_by_case(case_id)?;
    bindings
        .iter()
        .map(|binding| {
            let source_id = DataSourceId(binding.source_data_source_id.clone());
            let source = case_sources
                .iter()
                .find(|candidate| candidate.id == source_id)
                .ok_or_else(|| {
                    CephFsSourceError::InconsistentState(format!(
                        "CephFS parent source {} is not in the active case",
                        source_id.0
                    ))
                })?;
            if matches!(
                source.kind,
                DataSourceKind::CephFs | DataSourceKind::CephRbd
            ) {
                return Err(CephFsSourceError::InconsistentState(
                    "CephFS object replicas cannot be backed by a derived source".to_string(),
                ));
            }
            let storage = DataSourceRepo::new(case_conn)
                .find_storage(&source_id)?
                .ok_or_else(|| {
                    CephFsSourceError::InconsistentState(format!(
                        "CephFS parent source {} has no storage metadata",
                        source_id.0
                    ))
                })?;
            if !storage
                .platform
                .eq_ignore_ascii_case(DataSourcePlatform::Linux.as_storage_str())
                || !matches!(
                    storage.import_state.to_ascii_lowercase().as_str(),
                    "ready" | "ready_metadata"
                )
            {
                return Err(CephFsSourceError::InconsistentState(format!(
                    "CephFS parent source {} is not ready for object reads",
                    source_id.0
                )));
            }
            let path =
                crate::source_db::registered_source_db_path(case_conn, case_root, &source_id)?;
            CephFsObjectSource::new(source_id, binding.inventory_id.clone(), path)
                .map_err(|error| CephFsSourceError::InconsistentState(error.to_string()))
        })
        .collect()
}

fn descriptor_from_lineage(
    aggregate: &CephFsDerivedLineageAggregate,
) -> CephFsSourceResult<CephFsDescriptor> {
    let lineage = &aggregate.lineage;
    let pool_binding = |pool: &CephFsDerivedPoolRecord, role| CephFsPoolBinding {
        pool_id: pool.pool_id,
        role,
        provenance: pool
            .sources
            .iter()
            .map(|source| CephFsPoolProvenance {
                source_identity: source.source_data_source_id.clone(),
                inventory_identity: source.inventory_id.clone(),
            })
            .collect(),
    };
    let metadata = aggregate
        .pools
        .iter()
        .find(|pool| pool.role == "metadata")
        .ok_or(CephFsSourceError::InvalidInput("metadata pool is missing"))?;
    let data_pools = aggregate
        .pools
        .iter()
        .filter(|pool| pool.role == "data")
        .map(|pool| {
            pool_binding(
                pool,
                CephFsPoolRole::Data {
                    ordinal: pool.ordinal,
                },
            )
        })
        .collect();
    let provenance = aggregate
        .map_provenance
        .iter()
        .map(|item| {
            Ok(CephFsMapProvenance {
                source_identity: item.source_data_source_id.clone(),
                inventory_identity: item.inventory_id.clone(),
                captured_at: DateTime::<Utc>::from_str(&item.captured_at).map_err(|_| {
                    CephFsSourceError::InconsistentState(
                        "stored CephFS capture time is invalid".to_string(),
                    )
                })?,
                raw_fsmap_sha256: item.raw_fsmap_sha256.clone(),
                raw_mdsmap_sha256: item.raw_mdsmap_sha256.clone(),
            })
        })
        .collect::<CephFsSourceResult<Vec<_>>>()?;
    Ok(CephFsDescriptor {
        identity: lineage.filesystem_identity.clone(),
        cluster_identity: lineage.cluster_identity.clone(),
        filesystem_id: lineage.filesystem_id,
        name: lineage.filesystem_name.clone(),
        fsmap_epoch: lineage.fsmap_epoch,
        mdsmap_epoch: lineage.mdsmap_epoch,
        state: CephFsDescriptorState::Present,
        metadata_pool: pool_binding(metadata, CephFsPoolRole::Metadata),
        data_pools,
        rank_bindings: Vec::new(),
        daemons: Vec::new(),
        provenance,
    })
}

fn validate_text(value: &str) -> CephFsSourceResult<()> {
    if value.trim().is_empty() || value.contains('\0') {
        return Err(CephFsSourceError::InvalidInput(
            "identity is empty or contains a NUL byte",
        ));
    }
    Ok(())
}
