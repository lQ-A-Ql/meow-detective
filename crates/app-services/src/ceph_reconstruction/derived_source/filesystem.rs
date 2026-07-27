use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use domain::{CaseId, DataSource, DataSourceId, DataSourceKind};
use evidence_core::{EvidenceReader, FileSystemReader};
use persistence_sqlite::repositories::datasource_repo::DataSourceRepo;

use crate::{
    datasource_service::{
        self, ImageFilesystemCandidate, ImageFilesystemKind, ImageFilesystemProbe,
        ImageFilesystemSource, LvmLogicalVolumeIdentity, LvmPhysicalVolumeSource, PartitionRecord,
        PartitionStatus,
    },
    file_service,
};

use super::catalog_manifest::summarize_source_connection;
use super::{DerivedSourceError, DerivedSourceResult, MaterializedRbdSource};
use crate::ceph_reconstruction::{
    open_rbd_head_image, RadosReplicaSource, RbdEvidenceReader, RbdImageDescriptor,
    SharedRadosObjectProvider, SourceDbRadosObjectProvider, STRICT_RBD_REPLICA_COUNT,
};

struct RbdEnumerationContext<'a> {
    source_conn: &'a rusqlite::Connection,
    data_source: &'a DataSource,
    provider: &'a SharedRadosObjectProvider,
    descriptor: &'a RbdImageDescriptor,
    candidates: &'a [ImageFilesystemCandidate],
    placeholders: &'a HashMap<usize, domain::FileEntryId>,
    catalog_fingerprint: &'a str,
    cancel_token: &'a AtomicBool,
}

struct RbdCandidateContext<'a> {
    source_conn: &'a rusqlite::Connection,
    data_source_id: &'a DataSourceId,
    fs: &'a dyn FileSystemReader,
    root_name: &'a str,
    placeholders: &'a HashMap<usize, domain::FileEntryId>,
    candidate: &'a ImageFilesystemCandidate,
    catalog_fingerprint: &'a str,
    cancel_token: &'a AtomicBool,
}

pub(super) fn build_catalog_on_connection(
    source_conn: &rusqlite::Connection,
    case_id: &CaseId,
    data_source: &DataSource,
    replicas: &[RadosReplicaSource],
    descriptor: &RbdImageDescriptor,
    lineage_fingerprint: &str,
    cancel_token: &AtomicBool,
) -> DerivedSourceResult<MaterializedRbdSource> {
    DataSourceRepo::new(source_conn).upsert_source_local_metadata(case_id, data_source)?;
    let catalog_fingerprint = super::derived_catalog_fingerprint(lineage_fingerprint);

    let provider = SharedRadosObjectProvider::new(
        SourceDbRadosObjectProvider::new(
            replicas.to_vec(),
            descriptor.metadata.data_pool_id,
            Vec::new(),
            STRICT_RBD_REPLICA_COUNT,
        )
        .map_err(|error| DerivedSourceError::Reconstruction(error.to_string()))?,
    );
    ensure_not_cancelled(cancel_token)?;
    let probe_started = Instant::now();
    let mut probe = detect_rbd_probe(&provider, descriptor, cancel_token)?;
    tracing::info!(
        data_source_id = %data_source.id.0,
        elapsed_ms = probe_started.elapsed().as_millis(),
        "Ceph RBD filesystem probe completed"
    );
    let lvm_started = Instant::now();
    expand_rbd_lvm_candidates(&mut probe, &provider, descriptor, cancel_token)?;
    tracing::info!(
        data_source_id = %data_source.id.0,
        elapsed_ms = lvm_started.elapsed().as_millis(),
        candidates = probe.candidates.len(),
        "Ceph RBD LVM expansion completed"
    );
    if probe.candidates.is_empty() {
        return Err(DerivedSourceError::NoFilesystem(
            descriptor.metadata.id.clone(),
        ));
    }
    file_service::store_data_source_partitions(source_conn, &data_source.id, &probe.partitions)
        .map_err(|error| {
            DerivedSourceError::Database(match error {
                file_service::FileServiceError::Db(error) => error,
                other => persistence_sqlite::DbError::System(other.to_string()),
            })
        })?;

    ensure_not_cancelled(cancel_token)?;
    let placeholders = seed_placeholders(
        source_conn,
        &data_source.id,
        &probe.partitions,
        cancel_token,
    )?;
    enumerate_rbd_candidates(RbdEnumerationContext {
        source_conn,
        data_source,
        provider: &provider,
        descriptor,
        candidates: &probe.candidates,
        placeholders: &placeholders,
        catalog_fingerprint: &catalog_fingerprint,
        cancel_token,
    })?;
    ensure_not_cancelled(cancel_token)?;
    summarize_source_connection(source_conn, data_source.clone())
}

fn enumerate_rbd_candidates(context: RbdEnumerationContext<'_>) -> DerivedSourceResult<()> {
    for candidate in context
        .candidates
        .iter()
        .filter(|candidate| candidate.kind != ImageFilesystemKind::LvmPool)
    {
        ensure_not_cancelled(context.cancel_token)?;
        let candidate_started = Instant::now();
        let fs = open_rbd_filesystem(
            context.provider,
            context.descriptor,
            candidate,
            context.cancel_token,
        )?;
        let open_elapsed = candidate_started.elapsed();
        let enumeration_started = Instant::now();
        let root_name = crate::import_pipeline::partition::format_partition_root_name(candidate);
        let stats = enumerate_rbd_candidate(RbdCandidateContext {
            source_conn: context.source_conn,
            data_source_id: &context.data_source.id,
            fs: fs.as_ref(),
            root_name: &root_name,
            placeholders: context.placeholders,
            candidate,
            catalog_fingerprint: context.catalog_fingerprint,
            cancel_token: context.cancel_token,
        })
        .map_err(|error| {
            if context.cancel_token.load(Ordering::Relaxed) {
                DerivedSourceError::ProcessingCancelled
            } else {
                DerivedSourceError::Database(error)
            }
        })?;
        tracing::info!(
            data_source_id = %context.data_source.id.0,
            partition_index = candidate.partition_index,
            filesystem = ?candidate.kind,
            open_elapsed_ms = open_elapsed.as_millis(),
            enumerate_elapsed_ms = enumeration_started.elapsed().as_millis(),
            files = stats.file_count,
            directories = stats.dir_count,
            "Ceph RBD filesystem candidate materialized"
        );
        ensure_catalog_complete(&stats)?;
        if !stats.warnings.is_empty() {
            tracing::warn!(
                data_source_id = %context.data_source.id.0,
                partition_index = candidate.partition_index,
                warning_count = stats.warnings.len(),
                "Ceph RBD filesystem candidate completed with localized metadata diagnostics"
            );
        }
    }
    Ok(())
}

fn enumerate_rbd_candidate(
    context: RbdCandidateContext<'_>,
) -> Result<file_service::EnumerationStats, persistence_sqlite::DbError> {
    let partition_index = context.candidate.partition_index.ok_or_else(|| {
        persistence_sqlite::DbError::System(
            "RBD filesystem candidate has no partition index".to_string(),
        )
    })?;
    let placeholder_id = context.placeholders.get(&partition_index).ok_or_else(|| {
        persistence_sqlite::DbError::System(format!(
            "RBD partition {partition_index} has no placeholder root"
        ))
    })?;
    let mut stats = file_service::replace_placeholder_root_checkpointed(
        context.source_conn,
        placeholder_id,
        context.fs,
        Some(context.root_name),
        None,
        context.cancel_token,
    )?;
    let locator_candidate =
        file_service::filesystem_locators::preview_candidate_for_locator(context.candidate)
            .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
    let locator_scope = file_service::filesystem_locators::derived_filesystem_locator_scope(
        context.catalog_fingerprint,
        &locator_candidate,
    )
    .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
    if let Err(error) = file_service::filesystem_locators::persist_filesystem_locators(
        context.source_conn,
        context.data_source_id,
        &locator_candidate,
        &locator_scope,
        context.fs,
    ) {
        tracing::warn!(
            data_source_id = %context.data_source_id.0,
            partition_index,
            error = %error,
            "RBD filesystem locator acceleration hints could not be persisted"
        );
        stats.warnings.push(format!(
            "Filesystem locator acceleration hints were not persisted: {error}"
        ));
    }
    Ok(stats)
}

pub(super) fn ensure_catalog_complete(
    stats: &file_service::EnumerationStats,
) -> DerivedSourceResult<()> {
    let diagnostic_count = stats.incomplete_catalog_diagnostic_count();
    if diagnostic_count == 0 {
        Ok(())
    } else {
        Err(DerivedSourceError::IncompleteCatalog {
            diagnostic_count,
            diagnostic_breakdown: catalog_diagnostic_breakdown(stats),
        })
    }
}

fn catalog_diagnostic_breakdown(stats: &file_service::EnumerationStats) -> String {
    let mut directory_partial = 0usize;
    let mut directory_unreadable = 0usize;
    let mut entry_unavailable = 0usize;
    for diagnostic in &stats.diagnostics {
        match diagnostic.kind {
            evidence_core::FileSystemDiagnosticKind::DirectoryPartial => directory_partial += 1,
            evidence_core::FileSystemDiagnosticKind::DirectoryUnreadable => {
                directory_unreadable += 1
            }
            evidence_core::FileSystemDiagnosticKind::EntryUnavailable => entry_unavailable += 1,
            evidence_core::FileSystemDiagnosticKind::MetadataDegraded
            | evidence_core::FileSystemDiagnosticKind::TypeConflict => {}
        }
    }
    format!(
        "directoryPartial={directory_partial}, directoryUnreadable={directory_unreadable}, entryUnavailable={entry_unavailable}"
    )
}

fn detect_rbd_probe(
    provider: &SharedRadosObjectProvider,
    descriptor: &RbdImageDescriptor,
    cancel_token: &AtomicBool,
) -> DerivedSourceResult<ImageFilesystemProbe> {
    ensure_not_cancelled(cancel_token)?;
    let mut reader = open_rbd_descriptor(provider, descriptor)?;
    let probe = datasource_service::detect_image_filesystem(&mut reader)
        .map_err(|error| DerivedSourceError::Reconstruction(error.to_string()))?;
    ensure_not_cancelled(cancel_token)?;
    Ok(probe)
}

fn expand_rbd_lvm_candidates(
    probe: &mut ImageFilesystemProbe,
    provider: &SharedRadosObjectProvider,
    descriptor: &RbdImageDescriptor,
    cancel_token: &AtomicBool,
) -> DerivedSourceResult<()> {
    ensure_not_cancelled(cancel_token)?;
    let pools = probe
        .candidates
        .iter()
        .filter(|candidate| candidate.kind == ImageFilesystemKind::LvmPool)
        .cloned()
        .collect::<Vec<_>>();
    if pools.is_empty() {
        return Ok(());
    }
    probe
        .candidates
        .retain(|candidate| candidate.kind != ImageFilesystemKind::LvmPool);
    probe
        .partitions
        .retain(|partition| partition.filesystem != Some(ImageFilesystemKind::LvmPool));
    let mut next_index = probe
        .partitions
        .iter()
        .map(|partition| partition.index)
        .max()
        .unwrap_or(0)
        + 1;

    for pool_candidate in pools {
        ensure_not_cancelled(cancel_token)?;
        expand_one_rbd_lvm_pool(
            probe,
            provider,
            descriptor,
            &pool_candidate,
            &mut next_index,
            cancel_token,
        )?;
    }
    Ok(())
}

fn expand_one_rbd_lvm_pool(
    probe: &mut ImageFilesystemProbe,
    provider: &SharedRadosObjectProvider,
    descriptor: &RbdImageDescriptor,
    pool_candidate: &ImageFilesystemCandidate,
    next_index: &mut usize,
    cancel_token: &AtomicBool,
) -> DerivedSourceResult<()> {
    let reader = open_rbd_descriptor(provider, descriptor)?;
    let pool = fs_lvm::LvmPool::discover(
        vec![Box::new(reader) as Box<dyn EvidenceReader>],
        vec![pool_candidate.offset],
    )
    .map_err(|error| DerivedSourceError::UnsupportedLvm(error.to_string()))?;
    if pool.physical_volume_offsets().len() > 1 {
        return Err(DerivedSourceError::UnsupportedLvm(
            "multi-PV RBD logical volumes are not enabled".to_string(),
        ));
    }
    let volumes = pool.list_readable_volumes();
    if volumes.is_empty() {
        return Err(DerivedSourceError::NoFilesystem(
            descriptor.metadata.id.clone(),
        ));
    }
    let pv_source = LvmPhysicalVolumeSource {
        source_path: format!("ceph-rbd://{}", descriptor.metadata.id),
        source_kind: Some(DataSourceKind::CephRbd),
        offset: pool_candidate.offset,
        pv_uuid: String::new(),
        pv_name: None,
    };
    for (volume_index, volume) in volumes {
        ensure_not_cancelled(cancel_token)?;
        let identity = LvmLogicalVolumeIdentity {
            vg_uuid: pool.volume_group().id.clone(),
            vg_name: pool.volume_group().name.clone(),
            lv_uuid: volume.uuid.clone(),
            lv_name: volume.name.clone(),
            pv_offsets: vec![pool_candidate.offset],
            pv_sources: vec![pv_source.clone()],
        };
        let mut lv_reader = pool
            .open_volume_reader(volume_index)
            .map_err(|error| DerivedSourceError::UnsupportedLvm(error.to_string()))?;
        let lv_probe = datasource_service::detect_image_filesystem(&mut lv_reader)
            .map_err(|error| DerivedSourceError::UnsupportedLvm(error.to_string()))?;
        let Some(fs_candidate) = lv_probe
            .candidates
            .iter()
            .find(|candidate| candidate.kind != ImageFilesystemKind::LvmPool)
        else {
            continue;
        };
        let partition_index = *next_index;
        *next_index += 1;
        let candidate = ImageFilesystemCandidate {
            partition_index: Some(partition_index),
            partition_name: Some(format!("{}/{}", identity.vg_name, identity.lv_name)),
            kind: fs_candidate.kind,
            offset: pool_candidate.offset,
            length: Some(volume.size_bytes),
            source: ImageFilesystemSource::LvmLogicalVolume,
            lvm_identity: Some(identity.clone()),
        };
        probe.candidates.push(candidate);
        probe.partitions.push(PartitionRecord {
            index: partition_index,
            name: format!("{} / {}", identity.vg_name, identity.lv_name),
            kind_label: format!("{:?}", fs_candidate.kind),
            type_guid: None,
            offset: pool_candidate.offset,
            length: volume.size_bytes,
            status: PartitionStatus::Supported,
            filesystem: Some(fs_candidate.kind),
            lvm_identity: Some(identity),
        });
    }
    Ok(())
}

fn open_rbd_filesystem(
    provider: &SharedRadosObjectProvider,
    descriptor: &RbdImageDescriptor,
    candidate: &ImageFilesystemCandidate,
    cancel_token: &AtomicBool,
) -> DerivedSourceResult<Box<dyn FileSystemReader + Send>> {
    ensure_not_cancelled(cancel_token)?;
    let (reader, offset) = if let Some(identity) = &candidate.lvm_identity {
        let base = open_rbd_descriptor(provider, descriptor)?;
        let pool = fs_lvm::LvmPool::discover(
            vec![Box::new(base) as Box<dyn EvidenceReader>],
            identity.pv_offsets.clone(),
        )
        .map_err(|error| DerivedSourceError::UnsupportedLvm(error.to_string()))?;
        let index = pool
            .list_volumes()
            .iter()
            .position(|volume| {
                (!identity.lv_uuid.is_empty() && volume.uuid == identity.lv_uuid)
                    || volume.name == identity.lv_name
            })
            .ok_or_else(|| DerivedSourceError::UnsupportedLvm(identity.lv_name.clone()))?;
        (
            pool.open_volume_reader(index)
                .map_err(|error| DerivedSourceError::UnsupportedLvm(error.to_string()))?,
            0,
        )
    } else {
        (
            Box::new(open_rbd_descriptor(provider, descriptor)?) as Box<dyn EvidenceReader>,
            candidate.offset,
        )
    };
    let fs: Box<dyn FileSystemReader + Send> = match candidate.kind {
        ImageFilesystemKind::Ext4 => Box::new(
            fs_ext4::Ext4Reader::open(reader, offset)
                .map_err(|error| DerivedSourceError::Reconstruction(error.to_string()))?,
        ),
        ImageFilesystemKind::Xfs => Box::new(
            fs_xfs::XfsReader::open(reader, offset)
                .map_err(|error| DerivedSourceError::Reconstruction(error.to_string()))?,
        ),
        ImageFilesystemKind::Btrfs => Box::new(
            fs_btrfs::BtrfsReader::open(reader, offset)
                .map_err(|error| DerivedSourceError::Reconstruction(error.to_string()))?,
        ),
        other => {
            return Err(DerivedSourceError::NoFilesystem(format!(
                "unsupported RBD filesystem {other:?}"
            )))
        }
    };
    ensure_not_cancelled(cancel_token)?;
    Ok(fs)
}

fn open_rbd_descriptor(
    provider: &SharedRadosObjectProvider,
    descriptor: &RbdImageDescriptor,
) -> DerivedSourceResult<RbdEvidenceReader> {
    open_rbd_head_image(descriptor, Box::new(provider.clone()))
        .map_err(|error| DerivedSourceError::Reconstruction(error.to_string()))
}

fn seed_placeholders(
    conn: &rusqlite::Connection,
    data_source_id: &DataSourceId,
    partitions: &[PartitionRecord],
    cancel_token: &AtomicBool,
) -> DerivedSourceResult<HashMap<usize, domain::FileEntryId>> {
    let mut placeholders = HashMap::new();
    for partition in partitions {
        ensure_not_cancelled(cancel_token)?;
        let root = file_service::insert_partition_placeholder_root(
            conn,
            data_source_id,
            partition.index,
            &partition.name,
            "supported",
        )?;
        placeholders.insert(partition.index, root);
    }
    Ok(placeholders)
}

fn ensure_not_cancelled(cancel_token: &AtomicBool) -> DerivedSourceResult<()> {
    if cancel_token.load(Ordering::Relaxed) {
        Err(DerivedSourceError::ProcessingCancelled)
    } else {
        Ok(())
    }
}
