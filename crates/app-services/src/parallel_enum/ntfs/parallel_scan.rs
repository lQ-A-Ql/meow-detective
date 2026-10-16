//! A bounded read/parse wave uses the shared Rayon pool; only the caller writes SQLite.
use super::batch_pipeline::{MftBatch, OrderedMftWindow};
use super::mft_scan::{MftScan, MFT_CHUNK_RECORDS};
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
    mut consume: impl FnMut(MftBatch, &mut PhaseRecorder) -> Result<(), ParallelEnumError>,
) -> Result<(), ParallelEnumError> {
    let total = scan.total_records();
    let mut extra = open_extra_readers(scan, factory, requested, profile)?;
    let workers = extra.len() + 1;
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
                let offset = start + slot as u64 * MFT_CHUNK_RECORDS;
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
            let result = consume(batch, profile);
            profile.record(Phase::MftWriterWall, writer_started, result_state(&result));
            result?;
        }
    }
    Ok(())
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
        .min(scan.total_records().div_ceil(MFT_CHUNK_RECORDS).max(1) as usize);
    let mut extra = Vec::new();
    for _ in 1..workers {
        let reader = profile.measure(Phase::MftReaderOpen, || {
            scan.reopen(factory.expect("multiple readers require a factory"))
        })?;
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
