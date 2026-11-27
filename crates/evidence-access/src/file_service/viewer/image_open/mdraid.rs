use crate::file_service::FileServiceError;
use domain::FileEntry;
use std::{io::Read, path::Path};

pub(crate) fn open_mdraid_file(
    source_path: &str,
    entry: &FileEntry,
    partition_index: Option<usize>,
) -> Result<Box<dyn Read>, FileServiceError> {
    open_mdraid_file_seekable(source_path, entry, partition_index)
        .map(|reader| reader as Box<dyn Read>)
}

pub(crate) fn open_mdraid_file_seekable(
    source_path: &str,
    entry: &FileEntry,
    partition_index: Option<usize>,
) -> Result<Box<dyn evidence_core::ReadSeek>, FileServiceError> {
    let filesystem = crate::mdraid::open_mdraid_partition(Path::new(source_path), partition_index)?;
    let size = entry
        .size
        .ok_or_else(|| FileServiceError::other("MD file size is missing"))?;
    crate::file_service::open_filesystem_range_file(filesystem, entry.path.clone(), size)
        .map_err(FileServiceError::Io)
}
