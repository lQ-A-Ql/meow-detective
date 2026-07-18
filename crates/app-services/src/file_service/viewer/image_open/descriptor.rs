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
    match open_candidate(source_path, candidate, &path_candidates, &mut open_reader) {
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
    open_descriptor_image_file(descriptor, |_: &Path| {
        context
            .open_evidence_reader(descriptor)
            .map_err(|error| std::io::Error::other(error.to_string()))
    })
}

fn open_candidate<F>(
    source_path: &Path,
    candidate: &PreviewPartitionCandidate,
    paths: &[String],
    open_reader: &mut F,
) -> std::io::Result<Option<RangeContentReader>>
where
    F: FnMut(&Path) -> std::io::Result<Box<dyn EvidenceReader>>,
{
    if candidate.filesystem_kind == "NTFS" {
        return open_ntfs(source_path, candidate, paths, open_reader).map(Some);
    }
    if is_linux_filesystem_kind(&candidate.filesystem_kind) {
        return open_linux_image_candidate(source_path, candidate, paths, open_reader).map(Some);
    }
    if is_fat_filesystem_kind(&candidate.filesystem_kind) {
        return open_fat_or_exfat_image_candidate(source_path, candidate, paths, open_reader)
            .map(Some);
    }
    try_open_exfat_image_candidate(source_path, candidate, paths, open_reader)
}

fn open_ntfs<F>(
    source_path: &Path,
    candidate: &PreviewPartitionCandidate,
    paths: &[String],
    open_reader: &mut F,
) -> std::io::Result<RangeContentReader>
where
    F: FnMut(&Path) -> std::io::Result<Box<dyn EvidenceReader>>,
{
    let (reader, fs_offset) = open_candidate_block_reader(source_path, candidate, open_reader)?;
    let fs = fs_ntfs::NtfsReader::open(reader, fs_offset)?;
    open_first_image_path_seekable(&fs, paths)
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
    let (reader, fs_offset) = open_candidate_block_reader(source_path, candidate, open_reader)?;
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
    open_reader: &mut F,
) -> std::io::Result<RangeContentReader>
where
    F: FnMut(&Path) -> std::io::Result<Box<dyn EvidenceReader>>,
{
    let (reader, fs_offset) = open_candidate_block_reader(source_path, candidate, open_reader)?;
    match candidate.filesystem_kind.as_str() {
        kind if kind.eq_ignore_ascii_case("ext4") => {
            open_first_image_path_seekable(&fs_ext4::Ext4Reader::open(reader, fs_offset)?, paths)
        }
        kind if kind.eq_ignore_ascii_case("xfs") => {
            open_first_image_path_seekable(&fs_xfs::XfsReader::open(reader, fs_offset)?, paths)
        }
        kind if kind.eq_ignore_ascii_case("btrfs") => {
            open_first_image_path_seekable(&fs_btrfs::BtrfsReader::open(reader, fs_offset)?, paths)
        }
        _ => Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            format!("unsupported Linux filesystem {}", candidate.filesystem_kind),
        )),
    }
}
