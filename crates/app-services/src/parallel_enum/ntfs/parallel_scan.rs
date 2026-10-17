//! A bounded read/parse wave uses the shared Rayon pool; only the caller writes SQLite.
use super::batch_pipeline::{MftBatch, OrderedMftWindow};
use super::mft_scan::{MftScan, MFT_READ_CHUNK_RECORDS};
use crate::import_pipeline::profile::{Counters, PhaseRecorder};
use crate::parallel_enum::error::ParallelEnumError;
use evidence_core::EvidenceReaderFactory;
use rayon::prelude::*;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;
use transport::dto::import_benchmark::{
    ImportBenchmarkPhaseDto as Phase, ImportBenchmarkStateDto as State,
};

pub(super) fn collect_batches(
    scan: &mut MftScan,
    factory: Option<&dyn EvidenceReaderFactory>,
    requested: usize,
    cancel: &AtomicBool,
    profile: &mut PhaseRecorder,
    mut consume: impl FnMut(&MftBatch, &mut PhaseRecorder) -> Result<(), ParallelEnumError>,
) -> Result<(), ParallelEnumError> {
    collect_batches_with_budget(
        scan,
        factory,
        requested,
        cancel,
        profile,
        256 * crate::resource_budget::MIB,
        &mut consume,
    )
}

pub(super) fn collect_batches_with_budget(
    scan: &mut MftScan,
    factory: Option<&dyn EvidenceReaderFactory>,
    requested: usize,
    cancel: &AtomicBool,
    profile: &mut PhaseRecorder,
    window_bytes: u64,
    mut consume: impl FnMut(&MftBatch, &mut PhaseRecorder) -> Result<(), ParallelEnumError>,
) -> Result<(), ParallelEnumError> {
    let desired = factory.map_or(1, |f| {
        let c = f.capabilities();
        if c.independent_cursor && c.stable_snapshot {
            requested.clamp(1, 4).min(c.max_concurrent_readers.max(1))
        } else {
            1
        }
    });
    let (budget_workers, reserved) = reserve_wave(scan, factory, desired, window_bytes, cancel)?;
    scan.resource_hold = Some(reserved);
    let chunk_records = scan.chunk_records;
    let total = scan.total_records();
    let mut extra = open_extra_readers(scan, factory, budget_workers, profile)?;
    let workers = extra.len() + 1;
    profile.record_duration(Phase::MftReadParseWall, None, State::NotApplicable);
    profile.count(
        Phase::MftReadParseWall,
        Counters {
            mft_readers: Some(workers as u64),
            buffered_bytes: Some(wave_bytes(scan, budget_workers)),
            actual_chunk_records: Some(chunk_records),
            ..Default::default()
        },
    );
    let mut readers: Vec<_> = std::iter::once(scan)
        .chain(extra.iter_mut())
        .map(|scan| (scan, profile.fork()))
        .collect();
    let window = OrderedMftWindow::new(workers);
    let mut start = 0;
    while start < total {
        if cancel.load(Ordering::Relaxed) {
            return Err(ParallelEnumError::Cancelled);
        }
        // Each wave contains at most `workers` chunks, including out-of-order results.
        // Reuse independent readers and their buffers across waves.
        let wave_started = profile.start();
        let results: Vec<_> = readers
            .par_iter_mut()
            .enumerate()
            .filter_map(|(slot, (scan, recorder))| {
                let offset = start + slot as u64 * chunk_records;
                (offset < total).then(|| read_batch(scan, offset, cancel, recorder))
            })
            .collect();
        record_wave(profile, wave_started, &results);
        for result in results {
            let batch = result?;
            if !window.reserve(batch.start_record) {
                return Err(ParallelEnumError::MftParams(
                    "MFT batch window is full".into(),
                ));
            }
            window
                .submit(batch)
                .map_err(|_| ParallelEnumError::MftParams("invalid MFT batch".into()))?;
        }
        while let Some(batch) = window.take_next() {
            if cancel.load(Ordering::Relaxed) {
                return Err(ParallelEnumError::Cancelled);
            }
            start += batch.scanned_records;
            let writer_started = profile.start();
            let result = consume(&batch, profile);
            profile.record(Phase::MftWriterWall, writer_started, result_state(&result));
            result?;
        }
    }
    Ok(())
}

fn wave_bytes(scan: &MftScan, workers: usize) -> u64 {
    workers as u64
        * (scan.chunk_records
            * (3 * u64::from(scan.record_size())
                + 2 * std::mem::size_of::<fs_ntfs::mft_scanner::MftRecord>() as u64)
            + 2 * u64::from(scan.record_size()))
}
type WaveReservation = (
    usize,
    std::sync::Arc<dyn evidence_core::reader::ReaderResourceHold>,
);

fn reserve_wave(
    scan: &mut MftScan,
    factory: Option<&dyn EvidenceReaderFactory>,
    requested: usize,
    limit: u64,
    cancel: &AtomicBool,
) -> Result<WaveReservation, ParallelEnumError> {
    use crate::resource_budget::{global_pool, Resources};
    let scope = factory.and_then(|f| f.reader_scope());
    let acquire =
        |bytes,
         workers|
         -> std::io::Result<std::sync::Arc<dyn evidence_core::reader::ReaderResourceHold>> {
            if cancel.load(Ordering::Acquire) {
                return Err(crate::resource_budget::ResourceError::Cancelled.into_io());
            }
            if let Some(scope) = &scope {
                return scope.acquire_work(bytes, workers);
            }
            global_pool()
                .acquire(
                    Resources {
                        bytes,
                        workers,
                        ..Default::default()
                    },
                    if workers == 1 { Some(cancel) } else { None },
                )
                .map(|p| {
                    std::sync::Arc::new(p)
                        as std::sync::Arc<dyn evidence_core::reader::ReaderResourceHold>
                })
                .map_err(crate::resource_budget::ResourceError::into_io)
        };
    let per_record = 3 * u64::from(scan.record_size())
        + 2 * std::mem::size_of::<fs_ntfs::mft_scanner::MftRecord>() as u64;
    scan.chunk_records = MFT_READ_CHUNK_RECORDS
        .min(limit.saturating_sub(2 * u64::from(scan.record_size())) / per_record);
    if scan.chunk_records == 0 {
        return Err(ParallelEnumError::MftResourceLimit(
            "MFT window cannot hold one record",
        ));
    }
    let mut workers =
        requested.min(scan.total_records().div_ceil(scan.chunk_records).max(1) as usize);
    while workers > 1 {
        let bytes = wave_bytes(scan, workers);
        if bytes <= limit {
            match acquire(bytes, workers as u64) {
                Ok(permit) => return Ok((workers, permit)),
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(error) => {
                    return Err(if cancel.load(Ordering::Acquire) {
                        ParallelEnumError::Cancelled
                    } else {
                        ParallelEnumError::Io(error)
                    })
                }
            }
        }
        workers -= 1;
    }
    acquire(wave_bytes(scan, 1), 1)
        .map(|permit| (1, permit))
        .map_err(|error| {
            if cancel.load(Ordering::Acquire) {
                ParallelEnumError::Cancelled
            } else {
                ParallelEnumError::Io(error)
            }
        })
}

fn open_extra_readers(
    scan: &MftScan,
    factory: Option<&dyn EvidenceReaderFactory>,
    requested: usize,
    profile: &mut PhaseRecorder,
) -> Result<Vec<MftScan>, ParallelEnumError> {
    let workers = factory
        .map_or(1, |factory| {
            let capabilities = factory.capabilities();
            if capabilities.independent_cursor && capabilities.stable_snapshot {
                requested
                    .clamp(1, 4)
                    .min(capabilities.max_concurrent_readers.max(1))
            } else {
                1
            }
        })
        .min(scan.total_records().div_ceil(MFT_READ_CHUNK_RECORDS).max(1) as usize);
    let mut extra = Vec::new();
    for _ in 1..workers {
        let reader = match profile.measure(Phase::MftReaderOpen, || {
            scan.reopen(factory.expect("multiple readers require a factory"))
        }) {
            Ok(reader) => reader,
            Err(ParallelEnumError::Io(error)) if error.kind() == std::io::ErrorKind::WouldBlock => {
                break
            }
            Err(error) => return Err(error),
        };
        profile.count(
            Phase::MftReaderOpen,
            Counters {
                opens: 1,
                ..Default::default()
            },
        );
        extra.push(reader);
    }
    Ok(extra)
}

fn result_state<T>(result: &Result<T, ParallelEnumError>) -> State {
    match result {
        Ok(_) => State::Completed,
        Err(ParallelEnumError::Cancelled) => State::Cancelled,
        Err(_) => State::Failed,
    }
}

fn record_wave(
    profile: &mut PhaseRecorder,
    started: Option<Instant>,
    results: &[Result<MftBatch, ParallelEnumError>],
) {
    if started.is_none() {
        return;
    }
    let states = results.iter().map(result_state).collect::<Vec<_>>();
    let state = if states.contains(&State::Failed) {
        State::Failed
    } else if states.contains(&State::Cancelled) {
        State::Cancelled
    } else {
        State::Completed
    };
    profile.record(Phase::MftReadParseWall, started, state);
    profile.count(
        Phase::MftReadParseWall,
        Counters {
            buffered_batches: Some(results.iter().filter(|result| result.is_ok()).count() as u64),
            buffered_records: Some(
                results
                    .iter()
                    .filter_map(|result| result.as_ref().ok())
                    .map(|batch| batch.records.len() as u64)
                    .sum(),
            ),
            ..Default::default()
        },
    );
}

fn read_batch(
    scan: &mut MftScan,
    start: u64,
    cancel: &AtomicBool,
    profile: &mut PhaseRecorder,
) -> Result<MftBatch, ParallelEnumError> {
    if cancel.load(Ordering::Relaxed) {
        return Err(ParallelEnumError::Cancelled);
    }
    let (records, scanned_records) = scan.read_chunk(start, profile)?;
    Ok(MftBatch {
        start_record: start,
        scanned_records,
        logical_bytes: scanned_records * scan.record_size() as u64,
        records,
    })
}

#[cfg(test)]
#[path = "../../../tests/unit/parallel_enum/parallel_scan.rs"]
mod tests;
