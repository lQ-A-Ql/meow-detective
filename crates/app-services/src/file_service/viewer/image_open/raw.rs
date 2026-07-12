use std::{io::Read, path::Path};

use domain::FileEntry;
use evidence_core::{EvidenceReader, FileSystemReader};

use crate::file_service::{
    viewer::{entry_image_path_candidates, looks_like_exfat_boot_sector, open_first_image_path},
    FileServiceError,
};

use super::lvm::open_lvm_logical_volume_reader;

pub(crate) fn open_raw_file(
    source_path: &str,
    entry: &FileEntry,
    expected_partition_index: Option<usize>,
) -> Result<Box<dyn Read>, FileServiceError> {
    let reader = evidence_core::RawImageReader::open(Path::new(source_path))?;
    open_raw_image_file(entry, reader, expected_partition_index)
}

fn open_raw_image_file<R>(
    entry: &FileEntry,
    mut reader: R,
    expected_partition_index: Option<usize>,
) -> Result<Box<dyn Read>, FileServiceError>
where
    R: EvidenceReader + Read + std::io::Seek + 'static,
{
    let mut probe =
        crate::datasource_service::detect_image_filesystem(&mut reader).map_err(|error| {
            FileServiceError::other(format!("Failed to detect RAW filesystem: {error}"))
        })?;
    let source_path = reader.info().path.clone();
    let source_kind = reader.info().kind.clone();
    let paths = entry_image_path_candidates(entry);
    if probe.candidates.is_empty() {
        return open_unpartitioned_exfat(
            &mut reader,
            &probe.partitions,
            &source_path,
            &paths,
            expected_partition_index,
        );
    }
    let data_source_kind = if source_kind.contains("E01") {
        domain::DataSourceKind::E01
    } else {
        domain::DataSourceKind::Raw
    };
    crate::datasource_service::expand_lvm_pool_candidates(
        &mut probe,
        &source_path,
        &data_source_kind,
    );
    let indices = crate::datasource_service::assign_effective_partition_indices(&probe.candidates);
    for (position, candidate) in probe.candidates.iter().enumerate() {
        let index =
            crate::datasource_service::effective_partition_index(candidate, position, &indices);
        if expected_partition_index.is_some_and(|expected| expected != index) {
            continue;
        }
        if let Some(reader) = try_open_candidate(
            entry,
            &source_path,
            &paths,
            candidate,
            index,
            &data_source_kind,
        )? {
            return Ok(reader);
        }
    }
    Err(FileServiceError::other(format!(
        "Cannot open RAW image file '{}' from any partition",
        entry.path
    )))
}

fn open_unpartitioned_exfat<R>(
    reader: &mut R,
    partitions: &[crate::datasource_service::PartitionRecord],
    source_path: &Path,
    paths: &[String],
    expected_partition_index: Option<usize>,
) -> Result<Box<dyn Read>, FileServiceError>
where
    R: EvidenceReader + Read + std::io::Seek,
{
    if expected_partition_index.is_none_or(|expected| expected == 0)
        && looks_like_exfat_boot_sector(reader, 0)?
    {
        return open_exfat_at(source_path, paths, 0);
    }
    for partition in partitions {
        if expected_partition_index.is_some_and(|expected| expected != partition.index) {
            continue;
        }
        if looks_like_exfat_boot_sector(reader, partition.offset)? {
            return open_exfat_at(source_path, paths, partition.offset);
        }
    }
    Err(FileServiceError::other(
        "No supported filesystem detected in RAW image",
    ))
}

fn open_exfat_at(
    source_path: &Path,
    paths: &[String],
    offset: u64,
) -> Result<Box<dyn Read>, FileServiceError> {
    let reader: Box<dyn EvidenceReader> =
        Box::new(evidence_core::RawImageReader::open(source_path)?);
    let fs = fs_exfat::ExfatReader::open(reader, offset)?;
    open_first_image_path(&fs, paths).map_err(FileServiceError::from)
}

fn try_open_candidate(
    entry: &FileEntry,
    source_path: &Path,
    paths: &[String],
    candidate: &crate::datasource_service::ImageFilesystemCandidate,
    partition_index: usize,
    source_kind: &domain::DataSourceKind,
) -> Result<Option<Box<dyn Read>>, FileServiceError> {
    use crate::datasource_service::ImageFilesystemKind;
    match candidate.kind {
        ImageFilesystemKind::Ntfs => {
            let (reader, offset) = candidate_reader(source_path, candidate, source_kind)?;
            Ok(fs_ntfs::NtfsReader::open(reader, offset)
                .ok()
                .and_then(|fs| fs.open_file(&entry.path).ok()))
        }
        ImageFilesystemKind::Fat => open_fat_candidate(
            entry,
            source_path,
            paths,
            candidate,
            partition_index,
            source_kind,
        ),
        ImageFilesystemKind::Ext4 => {
            open_linux_candidate::<fs_ext4::Ext4Reader>(source_path, paths, candidate, source_kind)
        }
        ImageFilesystemKind::Xfs => {
            open_linux_candidate::<fs_xfs::XfsReader>(source_path, paths, candidate, source_kind)
        }
        ImageFilesystemKind::Btrfs => open_linux_candidate::<fs_btrfs::BtrfsReader>(
            source_path,
            paths,
            candidate,
            source_kind,
        ),
        _ => Ok(None),
    }
}

trait OpenFilesystem: FileSystemReader + Sized {
    fn open_fs(reader: Box<dyn EvidenceReader>, offset: u64) -> std::io::Result<Self>;
}

impl OpenFilesystem for fs_ext4::Ext4Reader {
    fn open_fs(reader: Box<dyn EvidenceReader>, offset: u64) -> std::io::Result<Self> {
        Self::open(reader, offset)
    }
}

impl OpenFilesystem for fs_xfs::XfsReader {
    fn open_fs(reader: Box<dyn EvidenceReader>, offset: u64) -> std::io::Result<Self> {
        Self::open(reader, offset)
    }
}

impl OpenFilesystem for fs_btrfs::BtrfsReader {
    fn open_fs(reader: Box<dyn EvidenceReader>, offset: u64) -> std::io::Result<Self> {
        Self::open(reader, offset)
    }
}

fn open_linux_candidate<F: OpenFilesystem>(
    source_path: &Path,
    paths: &[String],
    candidate: &crate::datasource_service::ImageFilesystemCandidate,
    source_kind: &domain::DataSourceKind,
) -> Result<Option<Box<dyn Read>>, FileServiceError> {
    let (reader, offset) = candidate_reader(source_path, candidate, source_kind)?;
    Ok(F::open_fs(reader, offset)
        .ok()
        .and_then(|fs| open_first_image_path(&fs, paths).ok()))
}

fn open_fat_candidate(
    entry: &FileEntry,
    source_path: &Path,
    paths: &[String],
    candidate: &crate::datasource_service::ImageFilesystemCandidate,
    partition_index: usize,
    source_kind: &domain::DataSourceKind,
) -> Result<Option<Box<dyn Read>>, FileServiceError> {
    let (reader, offset) = candidate_reader(source_path, candidate, source_kind)?;
    if let Ok(fs) = fs_fat::FatReader::open(reader, offset) {
        return Ok(open_first_image_path(&fs, paths).ok());
    }
    tracing::warn!(
        path = %entry.path,
        partition_index,
        offset = candidate.offset,
        "RAW FAT open failed; trying exFAT"
    );
    let (reader, offset) = candidate_reader(source_path, candidate, source_kind)?;
    Ok(fs_exfat::ExfatReader::open(reader, offset)
        .ok()
        .and_then(|fs| open_first_image_path(&fs, paths).ok()))
}

fn candidate_reader(
    source_path: &Path,
    candidate: &crate::datasource_service::ImageFilesystemCandidate,
    source_kind: &domain::DataSourceKind,
) -> Result<(Box<dyn EvidenceReader>, u64), FileServiceError> {
    let reader: Box<dyn EvidenceReader> =
        Box::new(evidence_core::RawImageReader::open(source_path)?);
    match &candidate.lvm_identity {
        Some(identity) => {
            let preview_identity =
                crate::file_service::viewer::partition::preview_lvm_identity_from_datasource(
                    identity,
                );
            let mut open_reader = |path: &Path| match source_kind {
                domain::DataSourceKind::E01 => image_e01::E01Reader::open(path)
                    .map(|reader| Box::new(reader) as Box<dyn EvidenceReader>),
                _ => evidence_core::RawImageReader::open(path)
                    .map(|reader| Box::new(reader) as Box<dyn EvidenceReader>),
            };
            Ok((
                open_lvm_logical_volume_reader(source_path, &preview_identity, &mut open_reader)?,
                0,
            ))
        }
        None => Ok((reader, candidate.offset)),
    }
}
