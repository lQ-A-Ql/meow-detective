//! A bounded read/parse wave uses the shared Rayon pool; only the caller writes SQLite.
use super::batch_pipeline::{MftBatch, OrderedMftWindow};
use super::mft_scan::{MftScan, MFT_CHUNK_RECORDS};
use crate::import_pipeline::profile::{Counters, PhaseRecorder};
use crate::parallel_enum::error::ParallelEnumError;
use evidence_core::EvidenceReaderFactory;
use rayon::prelude::*;
use std::sync::atomic::{AtomicBool, Ordering};
use transport::dto::import_benchmark::ImportBenchmarkPhaseDto as Phase;

pub(super) fn collect_batches(
    scan: &mut MftScan,
    factory: Option<&dyn EvidenceReaderFactory>,
    requested: usize,
    cancel: &AtomicBool,
    profile: &mut PhaseRecorder,
    mut consume: impl FnMut(MftBatch, &mut PhaseRecorder) -> Result<(), ParallelEnumError>,
) -> Result<(), ParallelEnumError> {
    let total = scan.total_records();
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
        .min(total.div_ceil(MFT_CHUNK_RECORDS).max(1) as usize);
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
        let results: Vec<_> = readers
            .par_iter_mut()
            .enumerate()
            .filter_map(|(slot, (scan, recorder))| {
                let offset = start + slot as u64 * MFT_CHUNK_RECORDS;
                (offset < total).then(|| read_batch(scan, offset, cancel, recorder))
            })
            .collect();
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
            consume(batch, profile)?;
        }
    }
    Ok(())
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
