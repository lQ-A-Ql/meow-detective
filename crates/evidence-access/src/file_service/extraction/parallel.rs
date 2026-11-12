//! Bounded parallel source reads with ordered destination publication.

use std::{
    collections::BTreeMap,
    io::{SeekFrom, Write},
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

use crossbeam_channel::{
    bounded, Receiver, RecvTimeoutError, SendTimeoutError, Sender, TrySendError,
};
use sha2::{Digest, Sha256};

use crate::file_service::FileServiceError;

use super::copy::{sync_and_publish, ProgressReporter, StreamCopyResult};
use super::progress::FileExtractionProgressCallback;

const CHANNEL_POLL_INTERVAL: Duration = Duration::from_millis(50);

#[derive(Clone, Copy)]
struct ReadWork {
    index: u64,
    offset: u64,
    length: usize,
}

#[derive(Clone, Copy)]
struct CopyShape {
    source_size: u64,
    chunk_bytes: usize,
    max_in_flight: usize,
}

enum WorkerResult {
    Chunk { index: u64, bytes: Vec<u8> },
    Error { message: String },
}

pub(super) fn copy_parallel_readers_to_destination(
    mut readers: Vec<Box<dyn evidence_core::ReadSeek + Send>>,
    source_size: u64,
    chunk_bytes: usize,
    max_in_flight: usize,
    destination: &Path,
    overwrite: bool,
    progress: Option<FileExtractionProgressCallback<'_>>,
) -> Result<StreamCopyResult, FileServiceError> {
    validate_configuration(readers.len(), chunk_bytes, max_in_flight)?;
    validate_reader_lengths(&mut readers, source_size)?;
    let parent = destination.parent().ok_or_else(|| {
        FileServiceError::invalid_input("destinationPath must have a parent directory")
    })?;
    let mut temporary = tempfile::Builder::new()
        .prefix(".meow-detective-extract-")
        .suffix(".tmp")
        .tempfile_in(parent)?;
    let mut progress = ProgressReporter::new(progress, Some(source_size));
    let shape = CopyShape {
        source_size,
        chunk_bytes,
        max_in_flight,
    };
    let result = copy_parallel(readers, shape, temporary.as_file_mut(), &mut progress)?;
    progress.report_finalizing(result.bytes_written, Some(source_size));
    sync_and_publish(temporary, destination, overwrite)?;
    Ok(result)
}

fn validate_reader_lengths(
    readers: &mut [Box<dyn evidence_core::ReadSeek + Send>],
    source_size: u64,
) -> Result<(), FileServiceError> {
    for reader in readers {
        let stream_size = reader.seek(SeekFrom::End(0))?;
        reader.seek(SeekFrom::Start(0))?;
        if stream_size != source_size {
            return Err(FileServiceError::integrity(format!(
                "Parallel evidence stream size does not match catalog size: expected {source_size}, found {stream_size}"
            )));
        }
    }
    Ok(())
}

fn copy_parallel(
    readers: Vec<Box<dyn evidence_core::ReadSeek + Send>>,
    shape: CopyShape,
    output: &mut std::fs::File,
    progress: &mut ProgressReporter<'_>,
) -> Result<StreamCopyResult, FileServiceError> {
    let stop = Arc::new(AtomicBool::new(false));
    std::thread::scope(|scope| {
        let (work_tx, work_rx) = bounded(shape.max_in_flight);
        let (result_tx, result_rx) = bounded(shape.max_in_flight);
        let mut workers = Vec::with_capacity(readers.len());
        for (worker_id, reader) in readers.into_iter().enumerate() {
            let receiver = work_rx.clone();
            let sender = result_tx.clone();
            let worker_stop = Arc::clone(&stop);
            workers.push(scope.spawn(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    worker_loop(reader, receiver, sender.clone(), &worker_stop)
                }));
                if result.is_err() {
                    send_worker_result(
                        &sender,
                        WorkerResult::Error {
                            message: format!("parallel extraction worker {worker_id} panicked"),
                        },
                        &worker_stop,
                    );
                    worker_stop.store(true, Ordering::Release);
                }
            }));
        }
        drop(work_rx);
        drop(result_tx);

        let copy_result =
            coordinate_copy(work_tx.clone(), &result_rx, shape, output, progress, &stop);
        stop.store(true, Ordering::Release);
        drop(work_tx);
        for worker in workers {
            if worker.join().is_err() {
                return Err(FileServiceError::other(
                    "Parallel extraction worker terminated unexpectedly",
                ));
            }
        }
        copy_result
    })
}

fn worker_loop(
    mut reader: Box<dyn evidence_core::ReadSeek + Send>,
    receiver: Receiver<ReadWork>,
    sender: Sender<WorkerResult>,
    stop: &AtomicBool,
) {
    while !stop.load(Ordering::Acquire) {
        let work = match receiver.recv_timeout(CHANNEL_POLL_INTERVAL) {
            Ok(work) => work,
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => break,
        };
        match read_chunk(reader.as_mut(), work) {
            Ok(bytes) => {
                if !send_worker_result(
                    &sender,
                    WorkerResult::Chunk {
                        index: work.index,
                        bytes,
                    },
                    stop,
                ) {
                    break;
                }
            }
            Err(error) => {
                send_worker_result(
                    &sender,
                    WorkerResult::Error {
                        message: format!(
                            "parallel extraction read failed at offset {}: {error}",
                            work.offset
                        ),
                    },
                    stop,
                );
                stop.store(true, Ordering::Release);
                break;
            }
        }
    }
}

fn send_worker_result(
    sender: &Sender<WorkerResult>,
    mut message: WorkerResult,
    stop: &AtomicBool,
) -> bool {
    loop {
        match sender.send_timeout(message, CHANNEL_POLL_INTERVAL) {
            Ok(()) => return true,
            Err(SendTimeoutError::Timeout(returned)) if !stop.load(Ordering::Acquire) => {
                message = returned;
            }
            Err(SendTimeoutError::Timeout(_)) | Err(SendTimeoutError::Disconnected(_)) => {
                return false;
            }
        }
    }
}

fn read_chunk(
    reader: &mut (dyn evidence_core::ReadSeek + Send),
    work: ReadWork,
) -> std::io::Result<Vec<u8>> {
    reader.seek(SeekFrom::Start(work.offset))?;
    let mut bytes = vec![0_u8; work.length];
    reader.read_exact(&mut bytes)?;
    Ok(bytes)
}

fn coordinate_copy(
    work_tx: Sender<ReadWork>,
    result_rx: &Receiver<WorkerResult>,
    shape: CopyShape,
    output: &mut std::fs::File,
    progress: &mut ProgressReporter<'_>,
    stop: &AtomicBool,
) -> Result<StreamCopyResult, FileServiceError> {
    let total_chunks = shape.source_size.div_ceil(shape.chunk_bytes as u64);
    let mut next_schedule = 0_u64;
    let mut next_write = 0_u64;
    let mut bytes_written = 0_u64;
    let mut pending = BTreeMap::new();
    let mut hasher = Sha256::new();
    fill_work_window(
        &work_tx,
        &mut next_schedule,
        next_write,
        total_chunks,
        shape,
    )?;

    while next_write < total_chunks {
        let message = match result_rx.recv_timeout(CHANNEL_POLL_INTERVAL) {
            Ok(message) => message,
            Err(RecvTimeoutError::Timeout) if stop.load(Ordering::Acquire) => {
                return Err(FileServiceError::other(
                    "Parallel extraction stopped before all chunks completed",
                ));
            }
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => {
                return Err(FileServiceError::other(
                    "Parallel extraction result channel closed early",
                ));
            }
        };
        match message {
            WorkerResult::Chunk { index, bytes } => {
                validate_chunk(index, &bytes, shape, total_chunks)?;
                if pending.insert(index, bytes).is_some() {
                    return Err(FileServiceError::integrity(
                        "Parallel extraction returned a duplicate chunk",
                    ));
                }
            }
            WorkerResult::Error { message } => {
                return Err(FileServiceError::Io(std::io::Error::other(message)));
            }
        }

        while let Some(bytes) = pending.remove(&next_write) {
            output.write_all(&bytes)?;
            hasher.update(&bytes);
            bytes_written = bytes_written
                .checked_add(bytes.len() as u64)
                .ok_or_else(|| FileServiceError::integrity("Extracted byte count overflow"))?;
            next_write += 1;
            progress.report_copying(
                bytes_written,
                Some(shape.source_size),
                bytes_written == shape.source_size,
            );
            fill_work_window(
                &work_tx,
                &mut next_schedule,
                next_write,
                total_chunks,
                shape,
            )?;
        }
    }
    if bytes_written != shape.source_size {
        return Err(FileServiceError::integrity(format!(
            "Extracted byte count does not match catalog size: expected {}, wrote {bytes_written}",
            shape.source_size
        )));
    }
    Ok(StreamCopyResult {
        bytes_written,
        sha256: hex::encode(hasher.finalize()),
    })
}

fn fill_work_window(
    sender: &Sender<ReadWork>,
    next_schedule: &mut u64,
    next_write: u64,
    total_chunks: u64,
    shape: CopyShape,
) -> Result<(), FileServiceError> {
    while *next_schedule < total_chunks
        && next_schedule.saturating_sub(next_write) < shape.max_in_flight as u64
    {
        let offset = next_schedule
            .checked_mul(shape.chunk_bytes as u64)
            .ok_or_else(|| FileServiceError::integrity("Extraction chunk offset overflow"))?;
        let length = (shape.source_size - offset).min(shape.chunk_bytes as u64) as usize;
        let work = ReadWork {
            index: *next_schedule,
            offset,
            length,
        };
        match sender.try_send(work) {
            Ok(()) => *next_schedule += 1,
            Err(TrySendError::Full(_)) => break,
            Err(TrySendError::Disconnected(_)) => {
                return Err(FileServiceError::other(
                    "Parallel extraction work channel closed early",
                ));
            }
        }
    }
    Ok(())
}

fn validate_chunk(
    index: u64,
    bytes: &[u8],
    shape: CopyShape,
    total_chunks: u64,
) -> Result<(), FileServiceError> {
    if index >= total_chunks {
        return Err(FileServiceError::integrity(
            "Parallel extraction returned an out-of-range chunk",
        ));
    }
    let offset = index
        .checked_mul(shape.chunk_bytes as u64)
        .ok_or_else(|| FileServiceError::integrity("Extraction chunk offset overflow"))?;
    let expected = (shape.source_size - offset).min(shape.chunk_bytes as u64) as usize;
    if bytes.len() != expected {
        return Err(FileServiceError::integrity(format!(
            "Parallel extraction chunk size mismatch: expected {expected}, received {}",
            bytes.len()
        )));
    }
    Ok(())
}

fn validate_configuration(
    reader_count: usize,
    chunk_bytes: usize,
    max_in_flight: usize,
) -> Result<(), FileServiceError> {
    if reader_count < 2 || chunk_bytes == 0 || max_in_flight < reader_count {
        return Err(FileServiceError::invalid_input(
            "Invalid bounded parallel extraction configuration",
        ));
    }
    Ok(())
}

#[cfg(test)]
#[path = "../../../tests/unit/file_service/extraction/parallel.rs"]
mod tests;
