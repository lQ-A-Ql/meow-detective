use std::io;
use std::path::Path;

use domain::DataSourceKind;
use evidence_core::{EvidenceReader, FileSystemReader};
use persistence_sqlite::repositories::partition_repo::DataSourcePartitionRecord;

use crate::file_service::{
    open_candidate_block_reader_with_lvm_cache, open_host_evidence_reader,
    preview_partition_candidate_from_record, LvmPoolRequestCache,
};

use super::MountServiceError;

pub(crate) fn open_partition_filesystem(
    source_path: &Path,
    source_kind: &DataSourceKind,
    case_id: &str,
    partition: &DataSourcePartitionRecord,
) -> Result<Box<dyn FileSystemReader + Send>, MountServiceError> {
    let source_kind = source_kind.to_string();
    let candidate = preview_partition_candidate_from_record(partition);
    let mut lvm_cache = LvmPoolRequestCache::new();
    let mut open_reader = |path: &Path| {
        open_host_evidence_reader(&source_kind, path, case_id)
            .map_err(|error| io::Error::other(error.to_string()))
    };
    let (reader, offset) = open_candidate_block_reader_with_lvm_cache(
        source_path,
        &candidate,
        &mut open_reader,
        &mut lvm_cache,
    )
    .map_err(|error| MountServiceError::Reader(error.to_string()))?;
    open_filesystem(&candidate.filesystem_kind, reader, offset)
}

fn open_filesystem(
    kind: &str,
    reader: Box<dyn EvidenceReader>,
    offset: u64,
) -> Result<Box<dyn FileSystemReader + Send>, MountServiceError> {
    if kind.eq_ignore_ascii_case("ntfs") {
        return fs_ntfs::NtfsReader::open(reader, offset)
            .map(|filesystem| Box::new(filesystem) as Box<dyn FileSystemReader + Send>)
            .map_err(|error| MountServiceError::Reader(error.to_string()));
    }
    if kind.eq_ignore_ascii_case("fat")
        || kind.eq_ignore_ascii_case("fat12")
        || kind.eq_ignore_ascii_case("fat16")
        || kind.eq_ignore_ascii_case("fat32")
    {
        return fs_fat::FatReader::open(reader, offset)
            .map(|filesystem| Box::new(filesystem) as Box<dyn FileSystemReader + Send>)
            .map_err(|error| MountServiceError::Reader(error.to_string()));
    }
    if kind.eq_ignore_ascii_case("exfat") {
        return fs_exfat::ExfatReader::open(reader, offset)
            .map(|filesystem| Box::new(filesystem) as Box<dyn FileSystemReader + Send>)
            .map_err(|error| MountServiceError::Reader(error.to_string()));
    }
    if kind.eq_ignore_ascii_case("ext4") {
        return fs_ext4::Ext4Reader::open(reader, offset)
            .map(|filesystem| Box::new(filesystem) as Box<dyn FileSystemReader + Send>)
            .map_err(|error| MountServiceError::Reader(error.to_string()));
    }
    if kind.eq_ignore_ascii_case("xfs") {
        return fs_xfs::XfsReader::open(reader, offset)
            .map(|filesystem| Box::new(filesystem) as Box<dyn FileSystemReader + Send>)
            .map_err(|error| MountServiceError::Reader(error.to_string()));
    }
    if kind.eq_ignore_ascii_case("btrfs") {
        return fs_btrfs::BtrfsReader::open(reader, offset)
            .map(|filesystem| Box::new(filesystem) as Box<dyn FileSystemReader + Send>)
            .map_err(|error| MountServiceError::Reader(error.to_string()));
    }
    Err(MountServiceError::Unsupported(format!(
        "filesystem kind '{kind}' is not supported by the v1 read-only mount"
    )))
}
