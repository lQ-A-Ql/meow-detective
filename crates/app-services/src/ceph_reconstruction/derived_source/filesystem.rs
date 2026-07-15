use std::collections::HashMap;
use std::path::Path;
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
    source_db::{self, checkpoint_source_db},
};

use super::{DerivedSourceError, DerivedSourceResult, MaterializedRbdSource};
use crate::ceph_reconstruction::{
    open_rbd_head_image, RadosReplicaSource, RbdEvidenceReader, RbdImageDescriptor,
    SharedRadosObjectProvider, SourceDbRadosObjectProvider,
};

pub(super) fn build_and_enumerate_source(
    case_root: &Path,
    case_id: &CaseId,
    data_source: &DataSource,
    replicas: &[RadosReplicaSource],
    descriptor: &RbdImageDescriptor,
) -> DerivedSourceResult<MaterializedRbdSource> {
    let materialization_started = Instant::now();
    let source_conn = source_db::open_source_db(case_root, &data_source.id)?;
    DataSourceRepo::new(&source_conn).upsert_source_local_metadata(case_id, data_source)?;

    let provider = SharedRadosObjectProvider::new(
        SourceDbRadosObjectProvider::new(
            replicas.to_vec(),
            descriptor.metadata.data_pool_id,
            Vec::new(),
            replicas.len(),
        )
        .map_err(|error| DerivedSourceError::Reconstruction(error.to_string()))?,
    );
    let probe_started = Instant::now();
    let mut probe = detect_rbd_probe(&provider, descriptor)?;
    tracing::info!(
        data_source_id = %data_source.id.0,
        elapsed_ms = probe_started.elapsed().as_millis(),
        "Ceph RBD filesystem probe completed"
    );
    let lvm_started = Instant::now();
    expand_rbd_lvm_candidates(&mut probe, &provider, descriptor)?;
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
    file_service::store_data_source_partitions(&source_conn, &data_source.id, &probe.partitions)
        .map_err(|error| {
            DerivedSourceError::Database(match error {
                file_service::FileServiceError::Db(error) => error,
                other => persistence_sqlite::DbError::System(other.to_string()),
            })
        })?;

    let placeholders = seed_placeholders(&source_conn, &data_source.id, &probe.partitions)?;
    let summary = enumerate_rbd_candidates(
        &source_conn,
        data_source,
        &provider,
        descriptor,
        &probe.candidates,
        &placeholders,
    )?;
    let graph_started = Instant::now();
    file_service::populate_file_graph_for_data_source(&source_conn, &data_source.id)?;
    tracing::info!(
        data_source_id = %data_source.id.0,
        elapsed_ms = graph_started.elapsed().as_millis(),
        "Ceph RBD file graph projection completed"
    );
    let checkpoint_started = Instant::now();
    checkpoint_source_db(&source_conn)?;
    tracing::info!(
        data_source_id = %data_source.id.0,
        elapsed_ms = checkpoint_started.elapsed().as_millis(),
        total_elapsed_ms = materialization_started.elapsed().as_millis(),
        "Ceph RBD derived source materialization completed"
    );
    Ok(summary)
}

fn enumerate_rbd_candidates(
    source_conn: &rusqlite::Connection,
    data_source: &DataSource,
    provider: &SharedRadosObjectProvider,
    descriptor: &RbdImageDescriptor,
    candidates: &[ImageFilesystemCandidate],
    placeholders: &HashMap<usize, domain::FileEntryId>,
) -> DerivedSourceResult<MaterializedRbdSource> {
    let mut summary = MaterializedRbdSource {
        data_source: data_source.clone(),
        file_count: 0,
        directory_count: 0,
        total_size: 0,
    };
    for candidate in candidates
        .iter()
        .filter(|candidate| candidate.kind != ImageFilesystemKind::LvmPool)
    {
        let candidate_started = Instant::now();
        let fs = open_rbd_filesystem(provider, descriptor, candidate)?;
        let open_elapsed = candidate_started.elapsed();
        let enumeration_started = Instant::now();
        let root_name = crate::import_pipeline::partition::format_partition_root_name(candidate);
        let stats = crate::import_pipeline::partition::enumerate_partition_with_fs(
            source_conn,
            &data_source.id,
            fs.as_ref(),
            &root_name,
            placeholders,
            candidate,
            None,
        )?;
        tracing::info!(
            data_source_id = %data_source.id.0,
            partition_index = candidate.partition_index,
            filesystem = ?candidate.kind,
            open_elapsed_ms = open_elapsed.as_millis(),
            enumerate_elapsed_ms = enumeration_started.elapsed().as_millis(),
            files = stats.file_count,
            directories = stats.dir_count,
            "Ceph RBD filesystem candidate materialized"
        );
        summary.file_count += stats.file_count;
        summary.directory_count += stats.dir_count;
        summary.total_size += stats.total_size;
    }
    Ok(summary)
}

fn detect_rbd_probe(
    provider: &SharedRadosObjectProvider,
    descriptor: &RbdImageDescriptor,
) -> DerivedSourceResult<ImageFilesystemProbe> {
    let mut reader = open_rbd_descriptor(provider, descriptor)?;
    datasource_service::detect_image_filesystem(&mut reader)
        .map_err(|error| DerivedSourceError::Reconstruction(error.to_string()))
}

fn expand_rbd_lvm_candidates(
    probe: &mut ImageFilesystemProbe,
    provider: &SharedRadosObjectProvider,
    descriptor: &RbdImageDescriptor,
) -> DerivedSourceResult<()> {
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
            let partition_index = next_index;
            next_index += 1;
            let candidate = ImageFilesystemCandidate {
                partition_index: Some(partition_index),
                partition_name: Some(format!("{}/{}", identity.vg_name, identity.lv_name)),
                kind: fs_candidate.kind,
                offset: pool_candidate.offset,
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
    }
    Ok(())
}

fn open_rbd_filesystem(
    provider: &SharedRadosObjectProvider,
    descriptor: &RbdImageDescriptor,
    candidate: &ImageFilesystemCandidate,
) -> DerivedSourceResult<Box<dyn FileSystemReader + Send>> {
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
) -> DerivedSourceResult<HashMap<usize, domain::FileEntryId>> {
    let mut placeholders = HashMap::new();
    for partition in partitions {
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
