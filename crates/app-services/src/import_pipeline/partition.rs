use std::collections::HashMap;

use evidence_core::{EvidenceReader, FileSystemReader};
use image_e01::E01Reader;

use crate::datasource_service::{
    self, ImageFilesystemKind, ImageFilesystemSource, LvmLogicalVolumeIdentity,
    LvmPhysicalVolumeSource,
};
use crate::file_service;
use crate::parallel_enum;

/// Format a filesystem candidate into a stable root name.
pub fn format_partition_root_name(
    candidate: &datasource_service::ImageFilesystemCandidate,
) -> String {
    let fs_label = match candidate.kind {
        ImageFilesystemKind::Ntfs => "NTFS",
        ImageFilesystemKind::Fat => "FAT",
        ImageFilesystemKind::BitLocker => "BitLocker",
        ImageFilesystemKind::Ext4 => "Ext4",
        ImageFilesystemKind::Xfs => "XFS",
        ImageFilesystemKind::Btrfs => "Btrfs",
        ImageFilesystemKind::LvmPool => "LVM",
    };

    match candidate.partition_index {
        Some(index) => datasource_service::partition_display_name(
            index,
            fs_label,
            candidate.partition_name.as_deref(),
            None,
        ),
        None => {
            datasource_service::volume_display_name(fs_label, candidate.partition_name.as_deref())
        }
    }
}

/// Format a partition record into a stable root name.
pub fn format_partition_record_root_name(
    partition: &datasource_service::PartitionRecord,
) -> String {
    let name = partition.name.trim();
    if name.is_empty()
        || name.eq_ignore_ascii_case("unknown")
        || matches!(name, "/" | "\\" | "." | "..")
    {
        return datasource_service::partition_display_name(
            partition.index,
            &partition.kind_label,
            None,
            None,
        );
    }

    name.to_string()
}

/// Format a partition progress detail string.
pub fn format_partition_progress_detail(
    completed_partitions: u32,
    total_partitions: u32,
    partition_progress: u32,
    current_partition: &str,
    detail: &str,
) -> String {
    format!(
        "[partition-progress] {}|{}|{}|{}|{}",
        completed_partitions,
        total_partitions.max(1),
        partition_progress.min(100),
        current_partition,
        detail
    )
}

/// Enumerate a filesystem within a partition, handling placeholder root replacement.
pub fn enumerate_partition_with_fs(
    conn: &rusqlite::Connection,
    data_source_id: &domain::DataSourceId,
    fs: &dyn FileSystemReader,
    root_name: &str,
    placeholder_roots: &HashMap<usize, domain::FileEntryId>,
    candidate: &datasource_service::ImageFilesystemCandidate,
    progress_cb: Option<&dyn Fn(u32)>,
) -> persistence_sqlite::DbResult<file_service::EnumerationStats> {
    if let Some(partition_index) = candidate.partition_index {
        if let Some(placeholder_id) = placeholder_roots.get(&partition_index) {
            return file_service::replace_placeholder_root_with_real(
                conn,
                placeholder_id,
                fs,
                Some(root_name),
                progress_cb,
            );
        }
    }
    file_service::enumerate_filesystem_with_root_name(
        conn,
        data_source_id,
        fs,
        Some(root_name),
        progress_cb,
    )
}

/// Enumerate an image data source (E01/RAW) with partition detection.
pub fn enumerate_image_data_source<R>(
    conn: &rusqlite::Connection,
    data_source_id: &domain::DataSourceId,
    mut reader: R,
    mut progress: impl FnMut(u32, &str) -> Result<(), String>,
    app: Option<&tauri::AppHandle>,
    job_id: Option<&domain::JobId>,
) -> persistence_sqlite::DbResult<file_service::EnumerationStats>
where
    R: EvidenceReader + std::io::Read + std::io::Seek + 'static,
{
    let mut fs_probe = datasource_service::detect_image_filesystem(&mut reader)
        .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;
    let source_path = reader.info().path.clone();
    let source_kind = reader.info().kind.clone();

    // Expand LVM pools into per-LV candidates
    let ds_kind = if source_kind.eq_ignore_ascii_case("e01") {
        domain::DataSourceKind::E01
    } else {
        domain::DataSourceKind::Raw
    };
    datasource_service::expand_lvm_pool_candidates(&mut fs_probe, &source_path, &ds_kind);

    if fs_probe.candidates.is_empty() {
        return Ok(file_service::EnumerationStats {
            file_count: 0,
            dir_count: 0,
            total_size: 0,
            warnings: fs_probe.warnings,
        });
    }

    let mut total = file_service::EnumerationStats {
        file_count: 0,
        dir_count: 0,
        total_size: 0,
        warnings: fs_probe.warnings,
    };

    file_service::store_data_source_partitions(conn, data_source_id, &fs_probe.partitions)
        .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;

    let total_partitions = fs_probe.partitions.len().max(1);
    let mut placeholder_roots = HashMap::new();
    for (index, partition) in fs_probe.partitions.iter().enumerate() {
        let root_name = format_partition_record_root_name(partition);
        let detail = match partition.status {
            datasource_service::PartitionStatus::Supported => {
                format!("Detected {root_name}; queued for import")
            }
            datasource_service::PartitionStatus::Expanded => {
                format!("Detected {root_name}; expanded into logical volumes")
            }
            datasource_service::PartitionStatus::EncryptedBitLocker => {
                format!("Detected locked {root_name}")
            }
            datasource_service::PartitionStatus::Unsupported => {
                format!("Detected unsupported {root_name}")
            }
        };
        let stage_progress = 12 + (((index as u32) * 8) / total_partitions as u32);
        let progress_detail = if partition.status == datasource_service::PartitionStatus::Supported
        {
            format_partition_progress_detail(
                index as u32,
                total_partitions as u32,
                0,
                &root_name,
                &detail,
            )
        } else {
            detail
        };
        progress(stage_progress, &progress_detail)
            .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;
        if let (Some(app), Some(jid)) = (app, job_id) {
            let job_repo = persistence_sqlite::repositories::job_repo::JobRepo::new(conn);
            if let Err(e) = job_repo.update_partition_progress(
                jid,
                &root_name,
                index as u32,
                total_partitions as u32,
                0,
            ) {
                tracing::debug!("Failed to update partition progress: {}", e);
            }
            crate::import_pipeline::emit::emit_partition_progress(
                app,
                &jid.0,
                &root_name,
                index as u32,
                total_partitions as u32,
                0,
            );
        }
        let status = match partition.status {
            datasource_service::PartitionStatus::Supported => "queued",
            datasource_service::PartitionStatus::Expanded => "redirected",
            datasource_service::PartitionStatus::EncryptedBitLocker => "locked",
            datasource_service::PartitionStatus::Unsupported => "unsupported",
        };
        if partition.status == datasource_service::PartitionStatus::Expanded {
            continue;
        }
        let placeholder_id = file_service::insert_partition_placeholder_root(
            conn,
            data_source_id,
            partition.index,
            &root_name,
            status,
        )?;
        placeholder_roots.insert(partition.index, placeholder_id);
    }

    let total_candidates = fs_probe.candidates.len().max(1);
    for (index, candidate) in fs_probe.candidates.into_iter().enumerate() {
        let root_name = format_partition_root_name(&candidate);
        let stage_progress = 25 + (((index as u32) * 35) / total_candidates as u32);
        let stage_detail = match candidate.kind {
            ImageFilesystemKind::Ntfs => format!("Enumerating {root_name}"),
            ImageFilesystemKind::Fat => format!("Enumerating {root_name}"),
            ImageFilesystemKind::BitLocker => format!("Skipping locked {root_name}"),
            ImageFilesystemKind::Ext4 => format!("Enumerating {root_name}"),
            ImageFilesystemKind::Xfs => format!("Enumerating {root_name}"),
            ImageFilesystemKind::Btrfs => format!("Enumerating {root_name}"),
            ImageFilesystemKind::LvmPool => {
                format!("Discovering LVM logical volumes in {root_name}")
            }
        };
        let progress_detail = format_partition_progress_detail(
            index as u32,
            total_candidates as u32,
            5,
            &root_name,
            &stage_detail,
        );
        progress(stage_progress, &progress_detail)
            .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;
        if let (Some(app), Some(jid)) = (app, job_id) {
            let job_repo = persistence_sqlite::repositories::job_repo::JobRepo::new(conn);
            if let Err(e) = job_repo.update_partition_progress(
                jid,
                &root_name,
                index as u32,
                total_candidates as u32,
                0,
            ) {
                tracing::debug!("Failed to update partition progress: {}", e);
            }
            crate::import_pipeline::emit::emit_partition_progress(
                app,
                &jid.0,
                &root_name,
                index as u32,
                total_candidates as u32,
                0,
            );
        }
        // Create progress callback for partition-level progress updates
        let emit_progress = |pct: u32| {
            if let (Some(a), Some(j)) = (app, job_id) {
                let job_repo = persistence_sqlite::repositories::job_repo::JobRepo::new(conn);
                let overall =
                    25 + ((index as u32 * 35) + (pct * 35 / 100)) / total_candidates.max(1) as u32;
                let _ =
                    job_repo.update_progress(j, overall.min(65), &format!("{root_name} {pct}%"));
                crate::import_pipeline::emit::emit_partition_progress(
                    a,
                    &j.0,
                    &root_name,
                    index as u32,
                    total_candidates as u32,
                    pct,
                );
            }
        };
        let progress_cb: Option<&dyn Fn(u32)> = if app.is_some() && job_id.is_some() {
            Some(&emit_progress)
        } else {
            None
        };

        let stats = match candidate.kind {
            ImageFilesystemKind::Ntfs => {
                let (partition_reader, fs_offset) =
                    open_candidate_reader(&source_path, &ds_kind, &candidate).map_err(|e| {
                        persistence_sqlite::DbError::System(format!(
                            "open reader for partition '{}': {}",
                            root_name, e
                        ))
                    })?;
                let fs = fs_ntfs::NtfsReader::open(partition_reader, fs_offset)
                    .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;
                enumerate_partition_with_fs(
                    conn,
                    data_source_id,
                    &fs,
                    &root_name,
                    &placeholder_roots,
                    &candidate,
                    progress_cb,
                )?
            }
            ImageFilesystemKind::Fat => {
                let (partition_reader, fs_offset) =
                    open_candidate_reader(&source_path, &ds_kind, &candidate).map_err(|e| {
                        persistence_sqlite::DbError::System(format!(
                            "open reader for partition '{}': {}",
                            root_name, e
                        ))
                    })?;
                let fs = fs_fat::FatReader::open(partition_reader, fs_offset)
                    .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;
                enumerate_partition_with_fs(
                    conn,
                    data_source_id,
                    &fs,
                    &root_name,
                    &placeholder_roots,
                    &candidate,
                    progress_cb,
                )?
            }
            ImageFilesystemKind::Ext4 => {
                let (partition_reader, fs_offset) =
                    open_candidate_reader(&source_path, &ds_kind, &candidate).map_err(|e| {
                        persistence_sqlite::DbError::System(format!(
                            "open reader for partition '{}': {}",
                            root_name, e
                        ))
                    })?;
                let fs = fs_ext4::Ext4Reader::open(partition_reader, fs_offset)
                    .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;
                enumerate_partition_with_fs(
                    conn,
                    data_source_id,
                    &fs,
                    &root_name,
                    &placeholder_roots,
                    &candidate,
                    progress_cb,
                )?
            }
            ImageFilesystemKind::Xfs => {
                let (partition_reader, fs_offset) =
                    open_candidate_reader(&source_path, &ds_kind, &candidate).map_err(|e| {
                        persistence_sqlite::DbError::System(format!(
                            "open reader for partition '{}': {}",
                            root_name, e
                        ))
                    })?;
                let fs = fs_xfs::XfsReader::open(partition_reader, fs_offset)
                    .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;
                enumerate_partition_with_fs(
                    conn,
                    data_source_id,
                    &fs,
                    &root_name,
                    &placeholder_roots,
                    &candidate,
                    progress_cb,
                )?
            }
            ImageFilesystemKind::Btrfs => {
                let (partition_reader, fs_offset) =
                    open_candidate_reader(&source_path, &ds_kind, &candidate).map_err(|e| {
                        persistence_sqlite::DbError::System(format!(
                            "open reader for partition '{}': {}",
                            root_name, e
                        ))
                    })?;
                let fs = fs_btrfs::BtrfsReader::open(partition_reader, fs_offset)
                    .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;
                enumerate_partition_with_fs(
                    conn,
                    data_source_id,
                    &fs,
                    &root_name,
                    &placeholder_roots,
                    &candidate,
                    progress_cb,
                )?
            }
            ImageFilesystemKind::BitLocker => {
                if let (Some(app), Some(jid)) = (app, job_id) {
                    let job_repo = persistence_sqlite::repositories::job_repo::JobRepo::new(conn);
                    if let Err(e) = job_repo.update_partition_progress(
                        jid,
                        &root_name,
                        (index as u32) + 1,
                        total_candidates as u32,
                        100,
                    ) {
                        tracing::debug!("Failed to update BitLocker partition progress: {}", e);
                    }
                    crate::import_pipeline::emit::emit_partition_progress(
                        app,
                        &jid.0,
                        &root_name,
                        (index as u32) + 1,
                        total_candidates as u32,
                        100,
                    );
                }
                continue;
            }
            ImageFilesystemKind::LvmPool => {
                // LVM pools are expanded at probe time by expand_lvm_pool_candidates.
                // This arm exists for safety — if reached, skip gracefully.
                tracing::warn!(
                    "LvmPool reached enumeration phase unexpectedly for '{}' — \
                     expansion should have occurred at probe time. Skipping.",
                    root_name,
                );
                continue;
            }
        };
        total.file_count += stats.file_count;
        total.dir_count += stats.dir_count;
        total.total_size += stats.total_size;
        total.warnings.extend(stats.warnings);
        if let (Some(app), Some(jid)) = (app, job_id) {
            let job_repo = persistence_sqlite::repositories::job_repo::JobRepo::new(conn);
            if let Err(e) = job_repo.update_partition_progress(
                jid,
                &root_name,
                (index as u32) + 1,
                total_candidates as u32,
                100,
            ) {
                tracing::debug!("Failed to update partition progress: {}", e);
            }
            crate::import_pipeline::emit::emit_partition_progress(
                app,
                &jid.0,
                &root_name,
                (index as u32) + 1,
                total_candidates as u32,
                100,
            );
        }
        let completed_detail = format_partition_progress_detail(
            index as u32,
            total_candidates as u32,
            100,
            &root_name,
            &format!("Imported {root_name}"),
        );
        let completed_progress = stage_progress
            .saturating_add((35 / total_candidates as u32).max(1))
            .min(68);
        progress(completed_progress, &completed_detail)
            .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;
    }

    if !total.warnings.is_empty() {
        progress(
            60,
            &format!("Partition warnings: {}", total.warnings.join(" | ")),
        )
        .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;
    }

    Ok(total)
}

/// Build a PartitionWork item for a single partition using pre-computed probe results.
/// Does NOT re-probe the image — uses candidates from the initial detect_image_filesystem call.
pub fn build_partition_work(
    source_path: &std::path::Path,
    source_kind: &domain::DataSourceKind,
    partition_index: usize,
    partition_name: &str,
    fs_kind: &str,
    probe_candidates: &[datasource_service::ImageFilesystemCandidate],
) -> Option<parallel_enum::PartitionWork> {
    let index_map = datasource_service::assign_effective_partition_indices(probe_candidates);
    let candidate = probe_candidates
        .iter()
        .enumerate()
        .find(|(i, c)| {
            let idx = datasource_service::effective_partition_index(c, *i, &index_map);
            idx == partition_index
        })
        .map(|(_, c)| c)?;

    let (base_reader, fs_offset) =
        open_candidate_reader(source_path, source_kind, candidate).ok()?;

    let fs: Box<dyn FileSystemReader + Send> = match candidate.kind {
        ImageFilesystemKind::Ntfs => {
            Box::new(fs_ntfs::NtfsReader::open(base_reader, fs_offset).ok()?)
        }
        ImageFilesystemKind::Fat => Box::new(fs_fat::FatReader::open(base_reader, fs_offset).ok()?),
        ImageFilesystemKind::Ext4 => {
            Box::new(fs_ext4::Ext4Reader::open(base_reader, fs_offset).ok()?)
        }
        ImageFilesystemKind::Xfs => Box::new(fs_xfs::XfsReader::open(base_reader, fs_offset).ok()?),
        ImageFilesystemKind::Btrfs => {
            Box::new(fs_btrfs::BtrfsReader::open(base_reader, fs_offset).ok()?)
        }
        ImageFilesystemKind::LvmPool | ImageFilesystemKind::BitLocker => return None,
    };

    Some(parallel_enum::PartitionWork {
        index: partition_index,
        name: partition_name.to_string(),
        fs_kind: fs_kind.to_string(),
        fs,
        source_path: source_path.to_path_buf(),
        source_kind: format!("{:?}", source_kind),
        volume_offset: candidate.offset,
    })
}

fn open_lvm_physical_volume_readers(
    source_path: &std::path::Path,
    source_kind: &domain::DataSourceKind,
    identity: &LvmLogicalVolumeIdentity,
) -> Option<Vec<Box<dyn EvidenceReader>>> {
    if identity.pv_offsets.is_empty() {
        return None;
    }
    if identity.pv_offsets.len() > 1 && identity.pv_sources.len() != identity.pv_offsets.len() {
        tracing::warn!(
            pv_offsets = identity.pv_offsets.len(),
            pv_sources = identity.pv_sources.len(),
            vg = %identity.vg_name,
            lv = %identity.lv_name,
            "LVM identity is missing per-PV source paths; refusing multi-PV fallback"
        );
        return None;
    }

    let mut readers = Vec::with_capacity(identity.pv_offsets.len());
    for (index, pv_offset) in identity.pv_offsets.iter().enumerate() {
        let pv_source_path = identity
            .pv_sources
            .get(index)
            .map(|source| std::path::Path::new(&source.source_path))
            .unwrap_or(source_path);
        let pv_source_kind = identity
            .pv_sources
            .get(index)
            .and_then(|source| source.source_kind.as_ref())
            .unwrap_or(source_kind);
        let mut reader: Box<dyn EvidenceReader> = match pv_source_kind {
            domain::DataSourceKind::E01 => Box::new(E01Reader::open(pv_source_path).ok()?),
            domain::DataSourceKind::Raw => {
                Box::new(evidence_core::RawImageReader::open(pv_source_path).ok()?)
            }
            domain::DataSourceKind::LogicalDirectory => return None,
        };
        if let Some(source) = identity.pv_sources.get(index) {
            validate_import_lvm_pv_source(reader.as_mut(), *pv_offset, source)?;
        }
        readers.push(reader);
    }
    Some(readers)
}

fn validate_import_lvm_pv_source(
    reader: &mut dyn EvidenceReader,
    expected_offset: u64,
    expected_source: &LvmPhysicalVolumeSource,
) -> Option<()> {
    if expected_source.pv_uuid.is_empty() {
        return Some(());
    }

    let label = match fs_lvm::label::parse_pv_label(reader, expected_offset) {
        Ok(label) => label,
        Err(error) => {
            tracing::warn!(
                source = %datasource_service::lvm_source_fingerprint(&expected_source.source_path),
                offset = expected_offset,
                error = %error,
                "LVM import PV source label validation failed"
            );
            return None;
        }
    };
    let actual_uuid = datasource_service::normalize_lvm_uuid_for_match(&label.pv_uuid);
    let expected_uuid = datasource_service::normalize_lvm_uuid_for_match(&expected_source.pv_uuid);
    if actual_uuid != expected_uuid {
        tracing::warn!(
            source = %datasource_service::lvm_source_fingerprint(&expected_source.source_path),
            offset = expected_offset,
            expected = %expected_source.pv_uuid,
            actual = %label.pv_uuid,
            "LVM import PV source UUID mismatch"
        );
        return None;
    }

    Some(())
}

fn open_candidate_reader(
    source_path: &std::path::Path,
    source_kind: &domain::DataSourceKind,
    candidate: &datasource_service::ImageFilesystemCandidate,
) -> Result<(Box<dyn EvidenceReader>, u64), String> {
    if matches!(candidate.source, ImageFilesystemSource::LvmLogicalVolume) {
        let identity = candidate
            .lvm_identity
            .as_ref()
            .ok_or_else(|| "LVM logical volume candidate missing identity".to_string())?;
        let readers = open_lvm_physical_volume_readers(source_path, source_kind, identity)
            .ok_or_else(|| "failed to open LVM physical volume readers".to_string())?;
        let pool = fs_lvm::LvmPool::discover(readers, identity.pv_offsets.clone())
            .map_err(|e| e.to_string())?;
        let lv_idx = find_lvm_volume_index(&pool, identity)
            .ok_or_else(|| "LVM logical volume identity not found in pool".to_string())?;
        let lv_reader = pool.open_volume(lv_idx).map_err(|e| e.to_string())?;
        return Ok((Box::new(lv_reader), 0));
    }

    let reader: Box<dyn EvidenceReader> = if *source_kind == domain::DataSourceKind::E01 {
        Box::new(E01Reader::open(source_path).map_err(|e| e.to_string())?)
    } else {
        Box::new(evidence_core::RawImageReader::open(source_path).map_err(|e| e.to_string())?)
    };
    Ok((reader, candidate.offset))
}

fn find_lvm_volume_index(
    pool: &fs_lvm::LvmPool,
    identity: &LvmLogicalVolumeIdentity,
) -> Option<usize> {
    let volumes = pool.list_volumes();
    if !identity.lv_uuid.is_empty() {
        if let Some(index) = volumes.iter().position(|lv| lv.uuid == identity.lv_uuid) {
            return Some(index);
        }
    }

    volumes.iter().position(|lv| lv.name == identity.lv_name)
}
