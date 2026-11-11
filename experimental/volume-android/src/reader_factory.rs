use evidence_core::{EvidenceReader, FileSystemReader};
use fs_erofs::ErofsReader;
use fs_ext4::Ext4Reader;
use fs_f2fs::F2fsReader;

use crate::{AndroidFilesystemKind, Result, VolumeAndroidError};

/// Opens the filesystem reader selected by already-probed Android metadata.
///
/// The source is expected to expose the selected partition as its logical
/// address space. `volume_offset` remains available for raw partition readers
/// that keep the parent image address space intact.
pub fn open_filesystem_reader(
    source: Box<dyn EvidenceReader>,
    filesystem: AndroidFilesystemKind,
    volume_offset: u64,
) -> Result<Box<dyn FileSystemReader>> {
    filesystem.require_reader()?;
    match filesystem {
        AndroidFilesystemKind::Ext4 => Ext4Reader::open(source, volume_offset)
            .map(|reader| Box::new(reader) as Box<dyn FileSystemReader>)
            .map_err(|error| reader_error(filesystem, error)),
        AndroidFilesystemKind::F2fs => F2fsReader::open(source, volume_offset)
            .map(|reader| Box::new(reader) as Box<dyn FileSystemReader>)
            .map_err(|error| reader_error(filesystem, error)),
        AndroidFilesystemKind::Erofs => ErofsReader::open(source, volume_offset)
            .map(|reader| Box::new(reader) as Box<dyn FileSystemReader>)
            .map_err(|error| reader_error(filesystem, error)),
        AndroidFilesystemKind::Unknown => Err(VolumeAndroidError::UnrecognizedFilesystem),
    }
}

fn reader_error(
    filesystem: AndroidFilesystemKind,
    error: impl std::fmt::Display,
) -> VolumeAndroidError {
    VolumeAndroidError::FilesystemReaderOpen {
        filesystem,
        message: error.to_string(),
    }
}
