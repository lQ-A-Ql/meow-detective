use std::io::{Read, Write};
use std::path::Path;
use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};
use tempfile::NamedTempFile;

use crate::file_service::FileServiceError;

use super::progress::{
    FileExtractionProgressCallback, FileExtractionProgressPhase, FileExtractionProgressUpdate,
};

const COPY_BUFFER_SIZE: usize = 1024 * 1024;
const PROGRESS_BYTE_INTERVAL: u64 = 8 * 1024 * 1024;
const PROGRESS_TIME_INTERVAL: Duration = Duration::from_millis(100);

struct ProgressReporter<'a> {
    callback: Option<FileExtractionProgressCallback<'a>>,
    last_reported: u64,
    last_reported_at: Instant,
}

impl<'a> ProgressReporter<'a> {
    fn new(callback: Option<FileExtractionProgressCallback<'a>>, total_bytes: Option<u64>) -> Self {
        let mut reporter = Self {
            callback,
            last_reported: 0,
            last_reported_at: Instant::now(),
        };
        reporter.emit(FileExtractionProgressUpdate {
            phase: FileExtractionProgressPhase::Copying,
            bytes_written: 0,
            total_bytes,
        });
        reporter
    }

    fn report_copying(&mut self, bytes_written: u64, total_bytes: Option<u64>, force: bool) {
        if bytes_written == self.last_reported {
            return;
        }
        let byte_interval_reached =
            bytes_written.saturating_sub(self.last_reported) >= PROGRESS_BYTE_INTERVAL;
        let time_interval_reached = self.last_reported_at.elapsed() >= PROGRESS_TIME_INTERVAL;
        if !force && !byte_interval_reached && !time_interval_reached {
            return;
        }
        self.emit(FileExtractionProgressUpdate {
            phase: FileExtractionProgressPhase::Copying,
            bytes_written,
            total_bytes,
        });
        self.last_reported = bytes_written;
        self.last_reported_at = Instant::now();
    }

    fn report_finalizing(&mut self, bytes_written: u64, total_bytes: Option<u64>) {
        self.emit(FileExtractionProgressUpdate {
            phase: FileExtractionProgressPhase::Finalizing,
            bytes_written,
            total_bytes,
        });
    }

    fn emit(&mut self, update: FileExtractionProgressUpdate) {
        if let Some(callback) = self.callback.as_deref_mut() {
            callback(update);
        }
    }
}

#[derive(Debug)]
pub(crate) struct StreamCopyResult {
    pub(crate) bytes_written: u64,
    pub(crate) sha256: String,
}

pub(crate) fn copy_reader_to_destination(
    reader: &mut dyn Read,
    source_size: Option<u64>,
    destination: &Path,
    overwrite: bool,
    progress: Option<FileExtractionProgressCallback<'_>>,
) -> Result<StreamCopyResult, FileServiceError> {
    let parent = destination.parent().ok_or_else(|| {
        FileServiceError::invalid_input("destinationPath must have a parent directory")
    })?;
    let mut temporary = tempfile::Builder::new()
        .prefix(".meow-detective-extract-")
        .suffix(".tmp")
        .tempfile_in(parent)?;
    let mut progress = ProgressReporter::new(progress, source_size);
    let result = copy_and_hash(reader, temporary.as_file_mut(), source_size, &mut progress)?;
    progress.report_finalizing(result.bytes_written, source_size);
    sync_and_publish(temporary, destination, overwrite)?;
    Ok(result)
}

pub(crate) fn copy_chunks_to_destination<F>(
    source_size: u64,
    destination: &Path,
    overwrite: bool,
    progress: Option<FileExtractionProgressCallback<'_>>,
    mut read_chunk: F,
) -> Result<StreamCopyResult, FileServiceError>
where
    F: FnMut(u64, u32) -> Result<Vec<u8>, FileServiceError>,
{
    let parent = destination.parent().ok_or_else(|| {
        FileServiceError::invalid_input("destinationPath must have a parent directory")
    })?;
    let mut temporary = tempfile::Builder::new()
        .prefix(".meow-detective-extract-")
        .suffix(".tmp")
        .tempfile_in(parent)?;
    let mut hasher = Sha256::new();
    let mut offset = 0_u64;
    let mut progress = ProgressReporter::new(progress, Some(source_size));
    while offset < source_size {
        let requested = (source_size - offset).min(COPY_BUFFER_SIZE as u64) as u32;
        let bytes = read_chunk(offset, requested)?;
        if bytes.is_empty() {
            return Err(size_mismatch(source_size, offset));
        }
        if bytes.len() > requested as usize {
            return Err(FileServiceError::integrity(
                "Evidence range reader returned more bytes than requested",
            ));
        }
        temporary.as_file_mut().write_all(&bytes)?;
        hasher.update(&bytes);
        offset = offset.saturating_add(bytes.len() as u64);
        progress.report_copying(offset, Some(source_size), offset == source_size);
    }
    if offset != source_size {
        return Err(size_mismatch(source_size, offset));
    }
    let result = StreamCopyResult {
        bytes_written: offset,
        sha256: hex::encode(hasher.finalize()),
    };
    progress.report_finalizing(offset, Some(source_size));
    sync_and_publish(temporary, destination, overwrite)?;
    Ok(result)
}

fn copy_and_hash(
    reader: &mut dyn Read,
    output: &mut std::fs::File,
    source_size: Option<u64>,
    progress: &mut ProgressReporter<'_>,
) -> Result<StreamCopyResult, FileServiceError> {
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; COPY_BUFFER_SIZE];
    let mut bytes_written = 0_u64;
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        let next_bytes_written = bytes_written.saturating_add(read as u64);
        if source_size.is_some_and(|expected| next_bytes_written > expected) {
            return Err(size_mismatch(
                source_size.unwrap_or_default(),
                next_bytes_written,
            ));
        }
        output.write_all(&buffer[..read])?;
        hasher.update(&buffer[..read]);
        bytes_written = next_bytes_written;
        progress.report_copying(
            bytes_written,
            source_size,
            source_size == Some(bytes_written),
        );
    }
    if let Some(expected) = source_size {
        if bytes_written != expected {
            return Err(size_mismatch(expected, bytes_written));
        }
    }
    progress.report_copying(bytes_written, source_size, true);
    Ok(StreamCopyResult {
        bytes_written,
        sha256: hex::encode(hasher.finalize()),
    })
}

fn sync_and_publish(
    mut temporary: NamedTempFile,
    destination: &Path,
    overwrite: bool,
) -> Result<(), FileServiceError> {
    temporary.as_file_mut().flush()?;
    temporary.as_file().sync_all()?;
    let persisted = if overwrite {
        temporary.persist(destination)
    } else {
        temporary.persist_noclobber(destination)
    };
    persisted
        .map(|_| ())
        .map_err(|error| FileServiceError::Io(error.error))
}

fn size_mismatch(expected: u64, actual: u64) -> FileServiceError {
    FileServiceError::integrity(format!(
        "Extracted byte count does not match catalog size: expected {expected}, wrote {actual}"
    ))
}

#[cfg(test)]
#[path = "../../../tests/unit/file_service/extraction/copy.rs"]
mod tests;
