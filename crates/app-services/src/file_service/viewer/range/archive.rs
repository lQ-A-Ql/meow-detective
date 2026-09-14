use std::io::Read;
use std::path::Path;

use domain::FileEntry;
use evidence_core::FileSystemReader;

use crate::file_service::{
    viewer::{descriptor_file_entry, PreviewDescriptor, RangeContentReader},
    FileServiceError,
};

pub(super) fn open_descriptor_file(
    descriptor: &PreviewDescriptor,
) -> Result<RangeContentReader, FileServiceError> {
    open_file_seekable(&descriptor.source_path, &descriptor_file_entry(descriptor))
        .map(RangeContentReader::Seekable)
}

pub(super) fn open_descriptor_seekable(
    descriptor: &PreviewDescriptor,
) -> Result<Box<dyn evidence_core::ReadSeek>, FileServiceError> {
    open_file_seekable(&descriptor.source_path, &descriptor_file_entry(descriptor))
}

pub(super) fn open_file(
    source_path: &str,
    entry: &FileEntry,
) -> Result<Box<dyn Read>, FileServiceError> {
    Ok(open_reader(source_path)?.open_file(&entry.path)?)
}

pub(super) fn open_file_seekable(
    source_path: &str,
    entry: &FileEntry,
) -> Result<Box<dyn evidence_core::ReadSeek>, FileServiceError> {
    Ok(open_reader(source_path)?.open_file_seekable(&entry.path)?)
}

fn open_reader(
    source_path: &str,
) -> Result<std::sync::Arc<evidence_core::ArchiveFsReader>, FileServiceError> {
    let path = Path::new(source_path);
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("archive");
    Ok(evidence_core::ArchiveFsReader::open_cached(path, name)?)
}
