use super::telemetry::ScanTelemetry;
use super::{check_cancel, ContentError, SourceContent, CONTENT_CHUNK_BYTES};
use crossbeam_channel::{bounded, Receiver, SendTimeoutError};
use evidence_core::{EvidenceReader, SharedReaderFactory};
use sha2::{Digest, Sha256};
use std::{
    io::SeekFrom,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

const WAIT: Duration = Duration::from_millis(50);

#[derive(Debug)]
pub(crate) struct ScanResult {
    pub size: u64,
    pub digest: String,
    pub chunks: Vec<String>,
    pub workers: usize,
    pub peak_readers: usize,
    pub telemetry: ScanTelemetry,
}

struct Frame {
    index: u64,
    bytes: Vec<u8>,
    digest: String,
}

pub(crate) fn requested_workers() -> Result<usize, ContentError> {
    match std::env::var("FORENSICS_CONTENT_WORKERS") {
        Ok(value) => value
            .parse::<usize>()
            .ok()
            .filter(|value| *value >= 4)
            .ok_or(ContentError::InvalidWorkerCount),
        Err(std::env::VarError::NotPresent) => Ok(std::thread::available_parallelism()
            .map_or(4, usize::from)
            .max(4)),
        Err(_) => Err(ContentError::InvalidWorkerCount),
    }
}

pub(crate) fn scan(
    factory: &SharedReaderFactory,
    expected: Option<&SourceContent>,
    cancel: &AtomicBool,
    requested: usize,
) -> Result<ScanResult, ContentError> {
    let started = std::time::Instant::now();
    check_cancel(cancel)?;
    if requested < 4 {
        return Err(ContentError::InvalidWorkerCount);
    }
    factory.validate_snapshot()?;
    let size = factory.logical_size();
    if expected.is_some_and(|identity| identity.logical_size != size) {
        return Err(ContentError::Changed);
    }
    let chunks = size.div_ceil(CONTENT_CHUNK_BYTES as u64);
    let workers = requested.min(usize::try_from(chunks).unwrap_or(usize::MAX));
    let capabilities = factory.capabilities();
    if workers > 1
        && (!capabilities.independent_cursor || workers > capabilities.max_concurrent_readers)
    {
        return Err(ContentError::ParallelReaderUnsupported);
    }
    let bytes = (workers as u64)
        .checked_mul(3 * CONTENT_CHUNK_BYTES as u64)
        .and_then(|value| {
            value.checked_add(if expected.is_none() {
                SourceContent::estimated_bytes(size).ok()?
            } else {
                0
            })
        })
        .ok_or(ContentError::InvalidIdentity)?;
    let _memory = reserve(factory, bytes)?;
    let admission_started = std::time::Instant::now();
    let readers = (0..workers)
        .map(|_| factory.open_reader())
        .collect::<Result<Vec<_>, _>>()?;
    let telemetry = ScanTelemetry {
        admission_nanos: admission_started.elapsed().as_nanos() as u64,
        ..Default::default()
    };
    let abort = AtomicBool::new(false);
    let active = AtomicUsize::new(0);
    let peak = AtomicUsize::new(0);
    let result = execute(
        readers, workers, size, expected, cancel, &abort, &active, &peak, &telemetry,
    );
    let (digest, chunk_hashes) = result?;
    factory.validate_snapshot()?;
    let peak_readers = peak.load(Ordering::Acquire);
    let result = ScanResult {
        size,
        digest,
        chunks: chunk_hashes,
        workers,
        peak_readers,
        telemetry,
    };
    tracing::info!(
        workers = result.workers,
        peak_readers = result.peak_readers,
        logical_bytes = result.size,
        elapsed_ms = started.elapsed().as_millis() as u64,
        "Parallel content scan completed"
    );
    result.telemetry.report();
    Ok(result)
}

#[allow(clippy::too_many_arguments)]
fn execute(
    readers: Vec<Box<dyn EvidenceReader>>,
    workers: usize,
    size: u64,
    expected: Option<&SourceContent>,
    cancel: &AtomicBool,
    abort: &AtomicBool,
    active: &AtomicUsize,
    peak: &AtomicUsize,
    telemetry: &ScanTelemetry,
) -> Result<(String, Vec<String>), ContentError> {
    std::thread::scope(|scope| {
        let mut receivers = Vec::new();
        let mut threads = Vec::new();
        for (lane, reader) in readers.into_iter().enumerate() {
            let (sender, receiver) = bounded(1);
            let abort_ref = abort;
            let active_ref = active;
            let peak_ref = peak;
            match std::thread::Builder::new()
                .name(format!("content-scan-{lane}"))
                .spawn_scoped(scope, move || {
                    produce(
                        reader,
                        lane as u64,
                        workers as u64,
                        size,
                        cancel,
                        abort_ref,
                        active_ref,
                        peak_ref,
                        sender,
                        telemetry,
                    )
                }) {
                Ok(thread) => {
                    threads.push(thread);
                    receivers.push(receiver);
                }
                Err(error) => {
                    abort.store(true, Ordering::Release);
                    return Err(ContentError::Io(error));
                }
            }
        }
        let result = consume(
            &receivers,
            size.div_ceil(CONTENT_CHUNK_BYTES as u64),
            expected,
            cancel,
            telemetry,
        );
        abort.store(true, Ordering::Release);
        for thread in threads {
            if thread.join().is_err() {
                return Err(ContentError::WorkerPanicked);
            }
        }
        result
    })
}

fn reserve(
    factory: &SharedReaderFactory,
    bytes: u64,
) -> Result<Arc<dyn evidence_core::reader::ReaderResourceHold>, ContentError> {
    if let Some(scope) = factory.reader_scope() {
        return Ok(scope.acquire_work(bytes, 0)?);
    }
    Ok(Arc::new(
        source_kernel::resource_budget::global_pool()
            .acquire(
                source_kernel::resource_budget::Resources {
                    bytes,
                    ..Default::default()
                },
                None,
            )
            .map_err(source_kernel::resource_budget::ResourceError::into_io)?,
    ))
}

#[allow(clippy::too_many_arguments)]
fn produce(
    mut reader: Box<dyn EvidenceReader>,
    lane: u64,
    stride: u64,
    size: u64,
    cancel: &AtomicBool,
    abort: &AtomicBool,
    active: &AtomicUsize,
    peak: &AtomicUsize,
    sender: crossbeam_channel::Sender<Result<Frame, ContentError>>,
    telemetry: &ScanTelemetry,
) {
    reader.configure_content_scan();
    let count = size.div_ceil(CONTENT_CHUNK_BYTES as u64);
    let mut index = lane;
    while index < count && !abort.load(Ordering::Acquire) {
        let current = active.fetch_add(1, Ordering::AcqRel) + 1;
        peak.fetch_max(current, Ordering::AcqRel);
        let result = read_frame(&mut *reader, index, size, cancel, telemetry);
        active.fetch_sub(1, Ordering::AcqRel);
        let failed = result.is_err();
        let mut pending = result;
        let send_started = std::time::Instant::now();
        loop {
            if abort.load(Ordering::Acquire) {
                return;
            }
            match sender.send_timeout(pending, WAIT) {
                Ok(()) => break,
                Err(SendTimeoutError::Timeout(value)) => pending = value,
                Err(SendTimeoutError::Disconnected(_)) => return,
            }
        }
        ScanTelemetry::record(&telemetry.send_nanos, send_started);
        if failed {
            return;
        }
        index += stride;
    }
    telemetry
        .reader
        .lock()
        .expect("scan reader metrics lock")
        .merge(reader.read_metrics());
}

fn read_frame(
    reader: &mut dyn EvidenceReader,
    index: u64,
    size: u64,
    cancel: &AtomicBool,
    telemetry: &ScanTelemetry,
) -> Result<Frame, ContentError> {
    check_cancel(cancel)?;
    let admission_started = std::time::Instant::now();
    let _work = source_kernel::source_runtime::work_admission::global_admission()
        .acquire(domain::structure::ReadPriority::Deferred, cancel, None)
        .map_err(|error| {
            if cancel.load(Ordering::Acquire) {
                ContentError::Cancelled
            } else {
                ContentError::Io(error)
            }
        })?;
    ScanTelemetry::record(&telemetry.cpu_wait_nanos, admission_started);
    let offset = index * CONTENT_CHUNK_BYTES as u64;
    let read_started = std::time::Instant::now();
    reader.seek(SeekFrom::Start(offset))?;
    let mut bytes = vec![0; (size - offset).min(CONTENT_CHUNK_BYTES as u64) as usize];
    reader.read_exact(&mut bytes)?;
    ScanTelemetry::record(&telemetry.read_nanos, read_started);
    telemetry
        .logical_read_bytes
        .fetch_add(bytes.len() as u64, Ordering::Relaxed);
    check_cancel(cancel)?;
    let hash_started = std::time::Instant::now();
    let digest = hex::encode(Sha256::digest(&bytes));
    ScanTelemetry::record(&telemetry.chunk_hash_nanos, hash_started);
    Ok(Frame {
        index,
        bytes,
        digest,
    })
}

fn consume(
    receivers: &[Receiver<Result<Frame, ContentError>>],
    count: u64,
    expected: Option<&SourceContent>,
    cancel: &AtomicBool,
    telemetry: &ScanTelemetry,
) -> Result<(String, Vec<String>), ContentError> {
    let mut digest = Sha256::new();
    let mut chunks = Vec::new();
    for index in 0..count {
        let receiver = &receivers[index as usize % receivers.len()];
        let receive_started = std::time::Instant::now();
        let frame = loop {
            check_cancel(cancel)?;
            match receiver.recv_timeout(WAIT) {
                Ok(frame) => break frame?,
                Err(crossbeam_channel::RecvTimeoutError::Timeout) => continue,
                Err(_) => return Err(ContentError::WorkerPanicked),
            }
        };
        ScanTelemetry::record(&telemetry.receive_nanos, receive_started);
        if frame.index != index {
            return Err(ContentError::InvalidIdentity);
        }
        if expected.is_some_and(|identity| identity.chunk_sha256[index as usize] != frame.digest) {
            return Err(ContentError::Changed);
        }
        let hash_started = std::time::Instant::now();
        let _work = source_kernel::source_runtime::work_admission::global_admission()
            .acquire(domain::structure::ReadPriority::Deferred, cancel, None)
            .map_err(|error| {
                if cancel.load(Ordering::Acquire) {
                    ContentError::Cancelled
                } else {
                    ContentError::Io(error)
                }
            })?;
        digest.update(&frame.bytes);
        ScanTelemetry::record(&telemetry.ordered_hash_nanos, hash_started);
        telemetry.completed_frames.fetch_add(1, Ordering::Relaxed);
        if expected.is_none() {
            chunks.push(frame.digest);
        }
    }
    check_cancel(cancel)?;
    let digest = hex::encode(digest.finalize());
    if expected.is_some_and(|identity| identity.logical_sha256 != digest) {
        return Err(ContentError::Changed);
    }
    Ok((digest, chunks))
}

#[cfg(test)]
#[path = "../../../tests/unit/content_scan.rs"]
mod tests;
