use std::path::Path;

use evidence_core::EvidenceReader;

use crate::file_service::{
    viewer::{
        descriptor_image_path_candidates, exact_partition_candidate, is_exfat_filesystem_kind,
        is_fat_filesystem_kind, is_linux_filesystem_kind, looks_like_exfat_boot_sector,
        open_first_image_path_seekable, PreviewDescriptor, PreviewPartitionCandidate,
        PreviewReadContext, RangeContentReader,
    },
    FileServiceError,
};

use super::lvm::open_candidate_block_reader;
use super::ntfs::open_ntfs_descriptor_file;

pub(crate) fn open_descriptor_image_file<F>(
    descriptor: &PreviewDescriptor,
    mut open_reader: F,
) -> Result<RangeContentReader, FileServiceError>
where
    F: FnMut(&Path) -> std::io::Result<Box<dyn EvidenceReader>>,
{
    let candidate = exact_partition_candidate(descriptor)?;
    let source_path = Path::new(&descriptor.source_path);
    let path_candidates = descriptor_image_path_candidates(descriptor);
    match open_candidate(
        source_path,
        descriptor,
        candidate,
        &path_candidates,
        &mut open_reader,
    ) {
        Ok(Some(reader)) => return Ok(reader),
        Ok(None) => {}
        Err(error) => tracing::warn!(
            path = %descriptor.path,
            partition_index = candidate.partition_index,
            kind = %candidate.filesystem_kind,
            error = %error,
            "Descriptor file not found on exact partition"
        ),
    }
    Err(FileServiceError::other(format!(
        "Cannot open image-backed file '{}' from its exact partition",
        descriptor.path
    )))
}

pub(crate) fn open_descriptor_image_file_with_context<C>(
    context: &mut C,
    descriptor: &PreviewDescriptor,
) -> Result<RangeContentReader, FileServiceError>
where
    C: PreviewReadContext,
{
    let candidate = exact_partition_candidate(descriptor)?;
    let paths = descriptor_image_path_candidates(descriptor);
    match open_candidate_with_context(context, descriptor, candidate, &paths) {
        Ok(Some(reader)) => Ok(reader),
        Ok(None) => Err(FileServiceError::other(format!(
            "Cannot open image-backed file '{}' from its exact partition",
            descriptor.path
        ))),
        Err(error) => Err(error),
    }
}

fn open_candidate_with_context<C>(
    context: &mut C,
    descriptor: &PreviewDescriptor,
    candidate: &PreviewPartitionCandidate,
    paths: &[String],
) -> Result<Option<RangeContentReader>, FileServiceError>
where
    C: PreviewReadContext,
{
    let (mut reader, fs_offset, filesystem_kind) =
        context.open_candidate_block_reader(descriptor, candidate)?;
    if filesystem_kind.eq_ignore_ascii_case("ISO9660") {
        let reader = evidence_core::PartitionWindowReader::new(reader, fs_offset, None)?;
        let filesystem = evidence_core::Iso9660Reader::from_reader(
            Box::new(reader),
            Path::new(&descriptor.source_path)
                .file_name()
                .and_then(|name| name.to_str()),
        )?;
        return open_first_image_path_seekable(&filesystem, paths)
            .map(Some)
            .map_err(FileServiceError::Io);
    }
    if filesystem_kind == "NTFS" {
        return open_ntfs_descriptor_file(
            fs_ntfs::NtfsReader::open(reader, fs_offset)?,
            descriptor,
            candidate,
            paths,
        )
        .map(Some);
    }
    if is_linux_filesystem_kind(&filesystem_kind) {
        return open_context_linux_filesystem(
            reader,
            fs_offset,
            &filesystem_kind,
            paths,
            descriptor.size,
        )
        .map(Some)
        .map_err(FileServiceError::Io);
    }
    if is_fat_filesystem_kind(&filesystem_kind) {
        if looks_like_exfat_boot_sector(reader.as_mut(), fs_offset).unwrap_or(false) {
            return fs_exfat::ExfatReader::open(reader, fs_offset)
                .and_then(|filesystem| open_first_image_path_seekable(&filesystem, paths))
                .map(Some)
                .map_err(FileServiceError::Io);
        }
        return match fs_fat::FatReader::open(reader, fs_offset) {
            Ok(filesystem) => open_first_image_path_seekable(&filesystem, paths)
                .map(Some)
                .map_err(FileServiceError::Io),
            Err(fat_error) => {
                let (reader, fs_offset, _) =
                    context.open_candidate_block_reader(descriptor, candidate)?;
                match fs_exfat::ExfatReader::open(reader, fs_offset) {
                    Ok(filesystem) => open_first_image_path_seekable(&filesystem, paths)
                        .map(Some)
                        .map_err(FileServiceError::Io),
                    Err(exfat_error) => Err(FileServiceError::Io(std::io::Error::new(
                        exfat_error.kind(),
                        format!("FAT open failed: {fat_error}; exFAT open failed: {exfat_error}"),
                    ))),
                }
            }
        };
    }
    let mut reader = context.open_candidate_block_reader(descriptor, candidate)?;
    if !is_exfat_filesystem_kind(&reader.2)
        && !looks_like_exfat_boot_sector(reader.0.as_mut(), reader.1).unwrap_or(false)
    {
        return Ok(None);
    }
    let filesystem = fs_exfat::ExfatReader::open(reader.0, reader.1)?;
    open_first_image_path_seekable(&filesystem, paths)
        .map(Some)
        .map_err(FileServiceError::Io)
}

fn open_context_linux_filesystem(
    reader: Box<dyn EvidenceReader>,
    fs_offset: u64,
    filesystem_kind: &str,
    paths: &[String],
    size: u64,
) -> std::io::Result<RangeContentReader> {
    let filesystem = crate::file_service::open_block_filesystem(filesystem_kind, reader, fs_offset)
        .map_err(std::io::Error::other)?;
    for path in paths {
        match filesystem.read_file_range(path, 0, 0) {
            Ok(_) => {
                return crate::file_service::open_filesystem_range_file(
                    filesystem,
                    path.clone(),
                    size,
                )
                .map(RangeContentReader::Seekable)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error),
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::NotFound,
        "published file path was not found",
    ))
}

fn open_candidate<F>(
    source_path: &Path,
    descriptor: &PreviewDescriptor,
    candidate: &PreviewPartitionCandidate,
    paths: &[String],
    open_reader: &mut F,
) -> std::io::Result<Option<RangeContentReader>>
where
    F: FnMut(&Path) -> std::io::Result<Box<dyn EvidenceReader>>,
{
    if candidate.filesystem_kind.eq_ignore_ascii_case("ISO9660") {
        let (reader, fs_offset) = open_candidate_block_reader(source_path, candidate, open_reader)?;
        let reader = evidence_core::PartitionWindowReader::new(reader, fs_offset, None)?;
        let filesystem = evidence_core::Iso9660Reader::from_reader(
            Box::new(reader),
            source_path.file_name().and_then(|name| name.to_str()),
        )?;
        return open_first_image_path_seekable(&filesystem, paths).map(Some);
    }
    if candidate.filesystem_kind == "NTFS" {
        return open_ntfs(source_path, descriptor, candidate, paths, open_reader).map(Some);
    }
    if is_linux_filesystem_kind(&candidate.filesystem_kind) {
        return open_linux_image_candidate(
            source_path,
            candidate,
            paths,
            descriptor.size,
            open_reader,
        )
        .map(Some);
    }
    if is_fat_filesystem_kind(&candidate.filesystem_kind) {
        return open_fat_or_exfat_image_candidate(source_path, candidate, paths, open_reader)
            .map(Some);
    }
    try_open_exfat_image_candidate(source_path, candidate, paths, open_reader)
}

fn open_ntfs<F>(
    source_path: &Path,
    descriptor: &PreviewDescriptor,
    candidate: &PreviewPartitionCandidate,
    paths: &[String],
    open_reader: &mut F,
) -> std::io::Result<RangeContentReader>
where
    F: FnMut(&Path) -> std::io::Result<Box<dyn EvidenceReader>>,
{
    let (reader, fs_offset) = open_candidate_block_reader(source_path, candidate, open_reader)?;
    let fs = fs_ntfs::NtfsReader::open(reader, fs_offset)?;
    open_ntfs_descriptor_file(fs, descriptor, candidate, paths).map_err(|error| match error {
        FileServiceError::Io(error) => error,
        other => std::io::Error::other(other.to_string()),
    })
}

fn open_fat_or_exfat_image_candidate<F>(
    source_path: &Path,
    candidate: &PreviewPartitionCandidate,
    paths: &[String],
    open_reader: &mut F,
) -> std::io::Result<RangeContentReader>
where
    F: FnMut(&Path) -> std::io::Result<Box<dyn EvidenceReader>>,
{
    let (mut reader, fs_offset) = open_candidate_block_reader(source_path, candidate, open_reader)?;
    if looks_like_exfat_boot_sector(reader.as_mut(), fs_offset)? {
        let fs = fs_exfat::ExfatReader::open(reader, fs_offset)?;
        return open_first_image_path_seekable(&fs, paths);
    }
    match fs_fat::FatReader::open(reader, fs_offset) {
        Ok(fs) => open_first_image_path_seekable(&fs, paths),
        Err(fat_error) => {
            tracing::warn!(
                partition_index = candidate.partition_index,
                offset = candidate.offset,
                error = %fat_error,
                "Descriptor FAT open failed; trying exFAT"
            );
            let (reader, fs_offset) =
                open_candidate_block_reader(source_path, candidate, open_reader)?;
            match fs_exfat::ExfatReader::open(reader, fs_offset) {
                Ok(fs) => open_first_image_path_seekable(&fs, paths),
                Err(exfat_error) => Err(std::io::Error::new(
                    exfat_error.kind(),
                    format!("FAT open failed: {fat_error}; exFAT open failed: {exfat_error}"),
                )),
            }
        }
    }
}

fn try_open_exfat_image_candidate<F>(
    source_path: &Path,
    candidate: &PreviewPartitionCandidate,
    paths: &[String],
    open_reader: &mut F,
) -> std::io::Result<Option<RangeContentReader>>
where
    F: FnMut(&Path) -> std::io::Result<Box<dyn EvidenceReader>>,
{
    let (mut reader, fs_offset) = open_candidate_block_reader(source_path, candidate, open_reader)?;
    if !is_exfat_filesystem_kind(&candidate.filesystem_kind)
        && !looks_like_exfat_boot_sector(reader.as_mut(), fs_offset).unwrap_or(false)
    {
        return Ok(None);
    }
    let fs = fs_exfat::ExfatReader::open(reader, fs_offset)?;
    open_first_image_path_seekable(&fs, paths).map(Some)
}

pub(super) fn open_linux_image_candidate<F>(
    source_path: &Path,
    candidate: &PreviewPartitionCandidate,
    paths: &[String],
    size: u64,
    open_reader: &mut F,
) -> std::io::Result<RangeContentReader>
where
    F: FnMut(&Path) -> std::io::Result<Box<dyn EvidenceReader>>,
{
    let (reader, fs_offset) = open_candidate_block_reader(source_path, candidate, open_reader)?;
    open_context_linux_filesystem(reader, fs_offset, &candidate.filesystem_kind, paths, size)
}
