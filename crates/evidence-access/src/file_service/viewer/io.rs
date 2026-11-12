use std::io::{Read, SeekFrom};

use crate::file_service::FileServiceError;

use super::RangeContentReader;

pub fn skip_reader_bytes(
    reader: &mut dyn Read,
    mut remaining: u64,
) -> Result<(), FileServiceError> {
    if remaining == 0 {
        return Ok(());
    }
    if remaining > 1024 * 1024 {
        tracing::warn!(
            bytes_to_skip = remaining,
            "Sequential byte skip for large offset; consider using a seekable reader"
        );
    }

    let mut buffer = vec![0u8; 1024 * 1024];
    while remaining > 0 {
        let chunk_len = remaining.min(buffer.len() as u64) as usize;
        let read = reader.read(&mut buffer[..chunk_len])?;
        if read == 0 {
            return Err(FileServiceError::other("Read offset exceeds file size"));
        }
        remaining -= read as u64;
    }
    Ok(())
}

pub(crate) fn read_seekable_range(
    reader: &mut dyn evidence_core::ReadSeek,
    offset: u64,
    length: usize,
) -> Result<Vec<u8>, FileServiceError> {
    reader.seek(SeekFrom::Start(offset))?;
    read_bounded(reader, length)
}

pub(crate) fn read_bounded(
    reader: &mut dyn Read,
    length: usize,
) -> Result<Vec<u8>, FileServiceError> {
    let mut bytes = Vec::with_capacity(length);
    reader.take(length as u64).read_to_end(&mut bytes)?;
    Ok(bytes)
}

pub(crate) fn open_first_image_path(
    fs: &dyn evidence_core::FileSystemReader,
    path_candidates: &[String],
) -> std::io::Result<Box<dyn Read>> {
    let mut last_error = None;
    for path in path_candidates {
        match fs.open_file(path) {
            Ok(reader) => return Ok(reader),
            Err(error) => last_error = Some(error),
        }
    }
    Err(no_path_error(last_error))
}

pub(crate) fn open_first_image_path_seekable(
    fs: &dyn evidence_core::FileSystemReader,
    path_candidates: &[String],
) -> std::io::Result<RangeContentReader> {
    let mut last_error = None;
    for path in path_candidates {
        match fs.open_file_seekable(path) {
            Ok(reader) => return Ok(RangeContentReader::Seekable(reader)),
            Err(error) if error.kind() == std::io::ErrorKind::Unsupported => {}
            Err(error) => last_error = Some(error),
        }
    }
    for path in path_candidates {
        match fs.open_file(path) {
            Ok(reader) => return Ok(RangeContentReader::Streaming(reader)),
            Err(error) => last_error = Some(error),
        }
    }
    Err(no_path_error(last_error))
}

fn no_path_error(last_error: Option<std::io::Error>) -> std::io::Error {
    last_error.unwrap_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::NotFound, "No preview path candidates")
    })
}
