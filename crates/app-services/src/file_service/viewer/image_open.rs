//! Open full file content from image-backed (E01/RAW) and logical data sources.

use crate::file_service::viewer::{
    descriptor_image_path_candidates, entry_image_path_candidates, is_exfat_filesystem_kind,
    is_fat_filesystem_kind, is_preview_image_filesystem_kind, looks_like_exfat_boot_sector,
    open_e01_reader_cached, open_first_image_path, open_first_image_path_seekable,
    partition::{is_previewable_partition_status, preview_partition_candidate_from_record},
    PreviewDescriptor, PreviewLvmIdentity, PreviewPartitionCandidate, RangeContentReader,
};
use crate::file_service::FileServiceError;
use domain::FileEntry;
use evidence_core::{EvidenceReader, FileSystemReader};
use rusqlite::Connection;
use std::io::Read;
use std::path::Path;

pub(crate) fn open_descriptor_image_file<F>(
    descriptor: &PreviewDescriptor,
    mut open_reader: F,
) -> Result<RangeContentReader, FileServiceError>
where
    F: FnMut(&Path) -> std::io::Result<Box<dyn EvidenceReader>>,
{
    if descriptor.partition_candidates.is_empty() {
        return Err(FileServiceError::other(format!(
            "Cannot open image-backed file '{}' without partition candidates",
            descriptor.path
        )));
    }

    let source_path = Path::new(&descriptor.source_path);
    let path_candidates = descriptor_image_path_candidates(descriptor);
    for candidate in &descriptor.partition_candidates {
        let result = if candidate.filesystem_kind == "NTFS" {
            let boxed_reader = open_reader(source_path)?;
            match fs_ntfs::NtfsReader::open(boxed_reader, candidate.offset) {
                Ok(fs) => open_first_image_path_seekable(&fs, &path_candidates),
                Err(e) => {
                    tracing::warn!(
                        path = %descriptor.path,
                        partition_index = candidate.partition_index,
                        offset = candidate.offset,
                        error = %e,
                        "Descriptor NTFS open failed"
                    );
                    continue;
                }
            }
        } else if crate::file_service::viewer::is_linux_filesystem_kind(&candidate.filesystem_kind)
        {
            open_linux_image_candidate(source_path, candidate, &path_candidates, &mut open_reader)
        } else if is_fat_filesystem_kind(&candidate.filesystem_kind) {
            open_fat_or_exfat_image_candidate(
                source_path,
                candidate,
                &path_candidates,
                &mut open_reader,
            )
        } else {
            match try_open_exfat_image_candidate(
                source_path,
                candidate,
                &path_candidates,
                &mut open_reader,
            ) {
                Ok(Some(reader)) => Ok(reader),
                Ok(None) => continue,
                Err(e) => {
                    tracing::warn!(
                        path = %descriptor.path,
                        partition_index = candidate.partition_index,
                        offset = candidate.offset,
                        error = %e,
                        "Descriptor exFAT open failed"
                    );
                    continue;
                }
            }
        };

        match result {
            Ok(reader) => return Ok(reader),
            Err(e) => {
                tracing::warn!(
                    path = %descriptor.path,
                    partition_index = candidate.partition_index,
                    kind = %candidate.filesystem_kind,
                    error = %e,
                    "Descriptor file not found on partition"
                );
            }
        }
    }

    Err(FileServiceError::other(format!(
        "Cannot open image-backed file '{}' from any partition",
        descriptor.path
    )))
}

pub(crate) fn open_fat_or_exfat_image_candidate<F>(
    source_path: &Path,
    candidate: &PreviewPartitionCandidate,
    path_candidates: &[String],
    open_reader: &mut F,
) -> std::io::Result<RangeContentReader>
where
    F: FnMut(&Path) -> std::io::Result<Box<dyn EvidenceReader>>,
{
    let fat_result = {
        let boxed_reader = open_reader(source_path)?;
        match fs_fat::FatReader::open(boxed_reader, candidate.offset) {
            Ok(fs) => open_first_image_path_seekable(&fs, path_candidates),
            Err(e) => Err(e),
        }
    };

    match fat_result {
        Ok(reader) => Ok(reader),
        Err(fat_error) => {
            tracing::warn!(
                partition_index = candidate.partition_index,
                offset = candidate.offset,
                error = %fat_error,
                "Descriptor FAT open failed; trying exFAT"
            );

            let boxed_reader = open_reader(source_path)?;
            match fs_exfat::ExfatReader::open(boxed_reader, candidate.offset) {
                Ok(fs) => open_first_image_path_seekable(&fs, path_candidates),
                Err(exfat_error) => Err(std::io::Error::new(
                    exfat_error.kind(),
                    format!("FAT open failed: {fat_error}; exFAT open failed: {exfat_error}"),
                )),
            }
        }
    }
}

pub(crate) fn try_open_exfat_image_candidate<F>(
    source_path: &Path,
    candidate: &PreviewPartitionCandidate,
    path_candidates: &[String],
    open_reader: &mut F,
) -> std::io::Result<Option<RangeContentReader>>
where
    F: FnMut(&Path) -> std::io::Result<Box<dyn EvidenceReader>>,
{
    let mut boxed_reader = open_reader(source_path)?;
    let looks_like_exfat = is_exfat_filesystem_kind(&candidate.filesystem_kind)
        || looks_like_exfat_boot_sector(boxed_reader.as_mut(), candidate.offset).unwrap_or(false);
    if !looks_like_exfat {
        return Ok(None);
    }

    match fs_exfat::ExfatReader::open(boxed_reader, candidate.offset) {
        Ok(fs) => open_first_image_path_seekable(&fs, path_candidates).map(Some),
        Err(error) => Err(error),
    }
}

pub(crate) fn open_raw_file(
    source_path: &str,
    entry: &FileEntry,
    expected_partition_index: Option<usize>,
) -> Result<Box<dyn Read>, FileServiceError> {
    let reader = evidence_core::RawImageReader::open(Path::new(source_path))?;
    // RAW 镜像：使用简单的 MBR/GPT 探测（不需要缓存的分区表）
    open_raw_image_file(entry, reader, expected_partition_index)
}

pub(crate) fn open_raw_image_file<R>(
    entry: &FileEntry,
    mut reader: R,
    expected_partition_index: Option<usize>,
) -> Result<Box<dyn Read>, FileServiceError>
where
    R: EvidenceReader + Read + std::io::Seek + 'static,
{
    let probe = crate::datasource_service::detect_image_filesystem(&mut reader)
        .map_err(|e| FileServiceError::other(format!("Failed to detect RAW filesystem: {e}")))?;
    let source_path = reader.info().path.clone();
    let source_kind = reader.info().kind.clone();
    let path_candidates = entry_image_path_candidates(entry);
    if probe.candidates.is_empty() {
        if expected_partition_index.is_none_or(|expected| expected == 0)
            && looks_like_exfat_boot_sector(&mut reader, 0)?
        {
            let boxed: Box<dyn EvidenceReader> =
                Box::new(evidence_core::RawImageReader::open(&source_path)?);
            let fs = fs_exfat::ExfatReader::open(boxed, 0)?;
            return open_first_image_path(&fs, &path_candidates)
                .map_err(|e| FileServiceError::other(format!("{e}")));
        }

        for partition in &probe.partitions {
            if expected_partition_index.is_some_and(|expected| partition.index != expected) {
                continue;
            }
            if !looks_like_exfat_boot_sector(&mut reader, partition.offset)? {
                continue;
            }

            let boxed: Box<dyn EvidenceReader> =
                Box::new(evidence_core::RawImageReader::open(&source_path)?);
            let fs = fs_exfat::ExfatReader::open(boxed, partition.offset)?;
            return open_first_image_path(&fs, &path_candidates)
                .map_err(|e| FileServiceError::other(format!("{e}")));
        }

        return Err(FileServiceError::other(
            "No supported filesystem detected in RAW image",
        ));
    }
    let mut probe = probe;
    let ds_kind = if source_kind.contains("E01") {
        domain::DataSourceKind::E01
    } else {
        domain::DataSourceKind::Raw
    };
    crate::datasource_service::expand_lvm_pool_candidates(&mut probe, &source_path, &ds_kind);
    let candidates =
        crate::datasource_service::assign_effective_partition_indices(&probe.candidates);
    for (ci, candidate) in probe.candidates.iter().enumerate() {
        let eff = crate::datasource_service::effective_partition_index(candidate, ci, &candidates);
        if let Some(expected) = expected_partition_index {
            if eff != expected {
                continue;
            }
        }
        let boxed: Box<dyn EvidenceReader> =
            Box::new(evidence_core::RawImageReader::open(&source_path)?);
        match candidate.kind {
            crate::datasource_service::ImageFilesystemKind::Ntfs => {
                if let Ok(fs) = fs_ntfs::NtfsReader::open(boxed, candidate.offset) {
                    if let Ok(r) = fs.open_file(&entry.path) {
                        return Ok(r);
                    }
                }
            }
            crate::datasource_service::ImageFilesystemKind::Fat => {
                match fs_fat::FatReader::open(boxed, candidate.offset) {
                    Ok(fs) => {
                        if let Ok(r) = open_first_image_path(&fs, &path_candidates) {
                            return Ok(r);
                        }
                    }
                    Err(e) => {
                        tracing::warn!(
                            path = %entry.path,
                            partition_index = eff,
                            offset = candidate.offset,
                            error = %e,
                            "RAW FAT open failed; trying exFAT"
                        );

                        let exfat_boxed: Box<dyn EvidenceReader> =
                            Box::new(evidence_core::RawImageReader::open(&source_path)?);
                        if let Ok(fs) = fs_exfat::ExfatReader::open(exfat_boxed, candidate.offset) {
                            if let Ok(r) = open_first_image_path(&fs, &path_candidates) {
                                return Ok(r);
                            }
                        }
                    }
                }
            }
            crate::datasource_service::ImageFilesystemKind::Ext4 => {
                let (reader, fs_offset) =
                    raw_candidate_reader(&source_path, candidate, boxed, &ds_kind)?;
                if let Ok(fs) = fs_ext4::Ext4Reader::open(reader, fs_offset) {
                    if let Ok(r) = open_first_image_path(&fs, &path_candidates) {
                        return Ok(r);
                    }
                }
            }
            crate::datasource_service::ImageFilesystemKind::Xfs => {
                let (reader, fs_offset) =
                    raw_candidate_reader(&source_path, candidate, boxed, &ds_kind)?;
                if let Ok(fs) = fs_xfs::XfsReader::open(reader, fs_offset) {
                    if let Ok(r) = open_first_image_path(&fs, &path_candidates) {
                        return Ok(r);
                    }
                }
            }
            crate::datasource_service::ImageFilesystemKind::Btrfs => {
                let (reader, fs_offset) =
                    raw_candidate_reader(&source_path, candidate, boxed, &ds_kind)?;
                if let Ok(fs) = fs_btrfs::BtrfsReader::open(reader, fs_offset) {
                    if let Ok(r) = open_first_image_path(&fs, &path_candidates) {
                        return Ok(r);
                    }
                }
            }
            _ => {}
        }
    }
    Err(FileServiceError::other(format!(
        "Cannot open RAW image file '{}' from any partition",
        entry.path
    )))
}

fn raw_candidate_reader(
    source_path: &Path,
    candidate: &crate::datasource_service::ImageFilesystemCandidate,
    boxed_reader: Box<dyn EvidenceReader>,
    source_kind: &domain::DataSourceKind,
) -> std::io::Result<(Box<dyn EvidenceReader>, u64)> {
    match &candidate.lvm_identity {
        Some(identity) => {
            let preview_identity =
                crate::file_service::viewer::partition::preview_lvm_identity_from_datasource(
                    identity,
                );
            let mut open_reader = |path: &Path| -> std::io::Result<Box<dyn EvidenceReader>> {
                match source_kind {
                    domain::DataSourceKind::E01 => image_e01::E01Reader::open(path)
                        .map(|reader| Box::new(reader) as Box<dyn EvidenceReader>),
                    _ => evidence_core::RawImageReader::open(path)
                        .map(|reader| Box::new(reader) as Box<dyn EvidenceReader>),
                }
            };
            let lv_reader =
                open_lvm_logical_volume_reader(source_path, &preview_identity, &mut open_reader)?;
            Ok((Box::new(lv_reader), 0))
        }
        None => Ok((boxed_reader, candidate.offset)),
    }
}

pub(crate) fn open_linux_image_candidate<F>(
    source_path: &Path,
    candidate: &PreviewPartitionCandidate,
    path_candidates: &[String],
    open_reader: &mut F,
) -> std::io::Result<RangeContentReader>
where
    F: FnMut(&Path) -> std::io::Result<Box<dyn EvidenceReader>>,
{
    let (boxed_reader, fs_offset) =
        open_candidate_block_reader(source_path, candidate, open_reader)?;

    match candidate.filesystem_kind.as_str() {
        kind if kind.eq_ignore_ascii_case("ext4") => {
            let fs = fs_ext4::Ext4Reader::open(boxed_reader, fs_offset)?;
            open_first_image_path_seekable(&fs, path_candidates)
        }
        kind if kind.eq_ignore_ascii_case("xfs") => {
            let fs = fs_xfs::XfsReader::open(boxed_reader, fs_offset)?;
            open_first_image_path_seekable(&fs, path_candidates)
        }
        kind if kind.eq_ignore_ascii_case("btrfs") => {
            let fs = fs_btrfs::BtrfsReader::open(boxed_reader, fs_offset)?;
            open_first_image_path_seekable(&fs, path_candidates)
        }
        _ => Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            format!("unsupported Linux filesystem {}", candidate.filesystem_kind),
        )),
    }
}

pub(crate) fn open_candidate_block_reader<F>(
    source_path: &Path,
    candidate: &PreviewPartitionCandidate,
    open_reader: &mut F,
) -> std::io::Result<(Box<dyn EvidenceReader>, u64)>
where
    F: FnMut(&Path) -> std::io::Result<Box<dyn EvidenceReader>>,
{
    match &candidate.lvm_identity {
        Some(identity) => {
            let lv_reader = open_lvm_logical_volume_reader(source_path, identity, open_reader)?;
            Ok((Box::new(lv_reader) as Box<dyn EvidenceReader>, 0))
        }
        None => open_reader(source_path).map(|reader| (reader, candidate.offset)),
    }
}

pub(crate) fn open_lvm_logical_volume_reader<F>(
    source_path: &Path,
    identity: &PreviewLvmIdentity,
    open_reader: &mut F,
) -> std::io::Result<fs_lvm::LvReader>
where
    F: FnMut(&Path) -> std::io::Result<Box<dyn EvidenceReader>>,
{
    if identity.pv_offsets.is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "LVM preview identity has no physical volume offsets",
        ));
    }

    let mut readers = Vec::with_capacity(identity.pv_offsets.len());
    for _ in &identity.pv_offsets {
        readers.push(open_reader(source_path)?);
    }

    let pool =
        fs_lvm::LvmPool::discover(readers, identity.pv_offsets.clone()).map_err(|error| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("LVM discovery failed for preview: {error}"),
            )
        })?;
    let lv_index = find_lvm_preview_volume_index(&pool, identity).ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!(
                "LVM logical volume not found for preview: {}/{}",
                identity.vg_name, identity.lv_name
            ),
        )
    })?;

    pool.open_volume(lv_index).map_err(|error| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("LVM logical volume open failed for preview: {error}"),
        )
    })
}

pub(crate) fn find_lvm_preview_volume_index(
    pool: &fs_lvm::LvmPool,
    identity: &PreviewLvmIdentity,
) -> Option<usize> {
    let volumes = pool.list_volumes();
    if !identity.lv_uuid.is_empty() {
        if let Some(index) = volumes.iter().position(|lv| lv.uuid == identity.lv_uuid) {
            return Some(index);
        }
    }

    volumes.iter().position(|lv| lv.name == identity.lv_name)
}

pub(crate) fn open_e01_file(
    conn: &Connection,
    source_path: &str,
    entry: &FileEntry,
    expected_partition_index: Option<usize>,
) -> Result<Box<dyn Read>, FileServiceError> {
    // 查询导入时已存储的分区元数据
    let part_repo = persistence_sqlite::repositories::partition_repo::PartitionRepo::new(conn);
    let partitions = part_repo
        .find_by_data_source(&entry.data_source_id.0)
        .map_err(|e| FileServiceError::other(format!("Failed to query partitions: {e}")))?;

    if partitions.is_empty() {
        return Err(FileServiceError::other(
            "No partition metadata found for this data source. Re-import the E01 image.",
        ));
    }

    // 如果路径只是裸文件名，说明 import 时父链重构失败
    if !entry.path.contains('/') && !entry.path.contains('\\') {
        return Err(FileServiceError::other(format!(
            "Cannot preview '{}': path reconstruction did not resolve the parent directory. Re-import.",
            entry.path
        )));
    }

    // 收集候选分区：优先匹配 expected_partition_index，回退到第一个非加密 NTFS
    let candidates_to_try: Vec<
        &persistence_sqlite::repositories::partition_repo::DataSourcePartitionRecord,
    > = match expected_partition_index {
        Some(expected) => partitions
            .iter()
            .filter(|p| {
                p.partition_index as usize == expected && is_previewable_partition_status(&p.status)
            })
            .collect(),
        None => {
            // Fallback: try the first non-encrypted NTFS partition for entries
            // whose parent chain could not be resolved (e.g., /Unresolved/ entries)
            let previewable: Vec<_> = partitions
                .iter()
                .filter(|p| {
                    is_previewable_partition_status(&p.status)
                        && is_preview_image_filesystem_kind(
                            p.filesystem.as_deref().unwrap_or(&p.kind_label),
                        )
                })
                .collect();
            if previewable.is_empty() {
                return Err(FileServiceError::other(
                    "Cannot determine which partition this file belongs to. Re-import the E01 image.",
                ));
            }
            previewable
        }
    };

    if candidates_to_try.is_empty() {
        return Err(FileServiceError::other(format!(
            "Partition index {} not found or is encrypted. Re-import.",
            expected_partition_index.unwrap_or(0)
        )));
    }

    let path_candidates = entry_image_path_candidates(entry);
    for target in &candidates_to_try {
        let fs_kind = target.filesystem.as_deref().unwrap_or(&target.kind_label);
        let exfat_hint = if is_exfat_filesystem_kind(fs_kind) {
            true
        } else if crate::file_service::viewer::is_linux_filesystem_kind(fs_kind)
            || target.lvm_pv_offsets_json.is_some()
        {
            false
        } else {
            let mut probe_reader = open_e01_reader_cached(Path::new(source_path), "")?;
            looks_like_exfat_boot_sector(&mut probe_reader, target.offset).unwrap_or(false)
        };

        let candidate = preview_partition_candidate_from_record(target);

        let result = match fs_kind {
            "NTFS" => {
                let reader = open_e01_reader_cached(Path::new(source_path), "")?;
                let boxed_reader: Box<dyn EvidenceReader> = Box::new(reader);
                match fs_ntfs::NtfsReader::open(boxed_reader, target.offset) {
                    Ok(fs) => fs
                        .open_file(&entry.path)
                        .or_else(|_| fs.open_file(&entry.id.0)),
                    Err(e) => {
                        tracing::warn!(
                            path = %entry.path,
                            partition = %target.name,
                            offset = %target.offset,
                            error = %e,
                            "E01 NTFS open failed"
                        );
                        continue;
                    }
                }
            }
            "FAT" | "FAT32" | "FAT16" | "FAT12" => {
                let reader = open_e01_reader_cached(Path::new(source_path), "")?;
                let boxed_reader: Box<dyn EvidenceReader> = Box::new(reader);
                match fs_fat::FatReader::open(boxed_reader, target.offset) {
                    Ok(fs) => open_first_image_path(&fs, &path_candidates),
                    Err(e) => {
                        tracing::warn!(
                            path = %entry.path,
                            partition = %target.name,
                            offset = %target.offset,
                            error = %e,
                            "E01 FAT open failed; trying exFAT"
                        );

                        let exfat_reader = open_e01_reader_cached(Path::new(source_path), "")?;
                        let exfat_boxed: Box<dyn EvidenceReader> = Box::new(exfat_reader);
                        match fs_exfat::ExfatReader::open(exfat_boxed, target.offset) {
                            Ok(fs) => open_first_image_path(&fs, &path_candidates),
                            Err(exfat_error) => Err(std::io::Error::new(
                                exfat_error.kind(),
                                format!("FAT open failed: {e}; exFAT open failed: {exfat_error}"),
                            )),
                        }
                    }
                }
            }
            _ if exfat_hint => {
                let reader = open_e01_reader_cached(Path::new(source_path), "")?;
                let boxed_reader: Box<dyn EvidenceReader> = Box::new(reader);
                match fs_exfat::ExfatReader::open(boxed_reader, target.offset) {
                    Ok(fs) => open_first_image_path(&fs, &path_candidates),
                    Err(e) => {
                        tracing::warn!(
                            path = %entry.path,
                            partition = %target.name,
                            offset = %target.offset,
                            error = %e,
                            "E01 exFAT open failed"
                        );
                        continue;
                    }
                }
            }
            _ if crate::file_service::viewer::is_linux_filesystem_kind(fs_kind) => {
                let mut open_reader = |path: &Path| {
                    open_e01_reader_cached(path, "")
                        .map(|reader| Box::new(reader) as Box<dyn EvidenceReader>)
                };
                open_linux_image_candidate(
                    Path::new(source_path),
                    &candidate,
                    &path_candidates,
                    &mut open_reader,
                )
                .map(range_content_reader_into_read)
            }
            _ => continue,
        };

        match &result {
            Ok(_) => return result.map_err(|e| FileServiceError::other(format!("{e}"))),
            Err(e) => {
                tracing::warn!(
                    path = %entry.path,
                    partition = %target.name,
                    kind = %fs_kind,
                    error = %e,
                    "E01 file not found on partition"
                );
            }
        }
    }

    Err(FileServiceError::other(format!(
        "Cannot open image-backed file '{}' from any partition",
        entry.path
    )))
}

fn range_content_reader_into_read(reader: RangeContentReader) -> Box<dyn Read> {
    match reader {
        RangeContentReader::Seekable(reader) => reader as Box<dyn Read>,
        RangeContentReader::Streaming(reader) => reader,
    }
}
