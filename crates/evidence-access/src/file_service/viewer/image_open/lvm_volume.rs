use crate::file_service::FileServiceError;
use domain::FileEntry;
use std::{io::Read, path::Path};

pub(crate) fn open_lvm_volume_file(
    source_id: &str,
    source_path: &str,
    entry: &FileEntry,
    partition_index: Option<usize>,
) -> Result<Box<dyn Read>, FileServiceError> {
    open_lvm_volume_file_seekable(source_id, source_path, entry, partition_index)
        .map(|reader| reader as Box<dyn Read>)
}

pub(crate) fn open_lvm_volume_file_seekable(
    source_id: &str,
    source_path: &str,
    entry: &FileEntry,
    partition_index: Option<usize>,
) -> Result<Box<dyn evidence_core::ReadSeek>, FileServiceError> {
    let filesystem =
        crate::lvm::open_lvm_partition(source_id, Path::new(source_path), partition_index)?;
    let size = entry
        .size
        .ok_or_else(|| FileServiceError::other("LVM file size is missing"))?;
    crate::file_service::open_filesystem_range_file(filesystem, entry.path.clone(), size)
        .map_err(FileServiceError::Io)
}
