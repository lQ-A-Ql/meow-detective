use super::*;
#[path = "parallel_scan/pressure.rs"]
mod pressure;
#[path = "parallel_scan/resource_grid.rs"]
mod resource_grid;
use crate::import_pipeline::ImportProfiler;
use std::io::{Cursor, Read, Seek, SeekFrom};
use std::time::Duration;

use crate::parallel_enum::tests::ntfs_fixture as fixture;

struct DelayedReader {
    bytes: Cursor<Vec<u8>>,
    info: evidence_core::reader::ReaderInfo,
}

impl Read for DelayedReader {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        std::thread::sleep(Duration::from_millis(5));
        self.bytes.read(buffer)
    }
}

impl Seek for DelayedReader {
    fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
        self.bytes.seek(position)
    }
}

impl evidence_core::EvidenceReader for DelayedReader {
    fn info(&self) -> &evidence_core::reader::ReaderInfo {
        &self.info
    }
}

#[test]
fn slow_reader_and_writer_are_visible_in_distinct_wall_windows() {
    let profiler = ImportProfiler::default();
    {
        let bytes = fixture::image(32);
        let reader = DelayedReader {
            info: evidence_core::reader::ReaderInfo {
                path: "test-memory".into(),
                size: bytes.len() as u64,
                kind: "raw".into(),
            },
            bytes: Cursor::new(bytes),
        };
        let mut recorder = PhaseRecorder::new(Some(profiler.clone()), Some(0));
        let mut scan = super::super::mft_scan::prepare_mft_scan_from_reader(
            Box::new(reader),
            0,
            &mut recorder,
        )
        .unwrap();
        collect_batches(
            &mut scan,
            None,
            1,
            &AtomicBool::new(false),
            &mut recorder,
            |batch, _| {
                assert_eq!(batch.scanned_records, 32);
                std::thread::sleep(Duration::from_millis(10));
                Ok(())
            },
        )
        .unwrap();
    }
    let report = profiler.report();
    let wave = report
        .samples
        .iter()
        .find(|s| s.phase == Phase::MftReadParseWall)
        .unwrap();
    let writer = report
        .samples
        .iter()
        .find(|s| s.phase == Phase::MftWriterWall)
        .unwrap();
    assert!(wave.elapsed_ns.unwrap() >= 5_000_000);
    assert!(writer.elapsed_ns.unwrap() >= 10_000_000);
    assert_eq!(wave.max_buffered_batches, Some(1));
    assert!(wave.max_buffered_records.is_some_and(|records| records > 0));
}

fn batch(start_record: u64) -> Result<MftBatch, ParallelEnumError> {
    Ok(MftBatch {
        start_record,
        scanned_records: 1,
        logical_bytes: 1024,
        records: Vec::new(),
    })
}

#[test]
fn wave_observation_counts_only_completed_payloads_and_preserves_failure_precedence() {
    let profiler = ImportProfiler::default();
    {
        let mut recorder = PhaseRecorder::new(Some(profiler.clone()), Some(0));
        let started = recorder.start();
        record_wave(
            &mut recorder,
            started,
            &[
                batch(0),
                batch(1),
                Err(ParallelEnumError::Cancelled),
                Err(ParallelEnumError::MftParams("injected read failure".into())),
            ],
        );
    }
    let report = profiler.report();
    let sample = report
        .samples
        .iter()
        .find(|s| s.phase == Phase::MftReadParseWall)
        .unwrap();
    assert_eq!(sample.state, State::Failed);
    assert_eq!(sample.max_buffered_batches, Some(2));
    assert_eq!(sample.max_buffered_records, Some(0));
    assert!(sample.inclusive);
    assert!(sample.elapsed_ns.is_some());
}

#[test]
fn cancelled_wave_does_not_report_a_successful_zero_duration_or_failure() {
    let profiler = ImportProfiler::default();
    {
        let mut recorder = PhaseRecorder::new(Some(profiler.clone()), Some(0));
        let started = recorder.start();
        record_wave(&mut recorder, started, &[Err(ParallelEnumError::Cancelled)]);
    }
    let report = profiler.report();
    let sample = &report.samples[0];
    assert_eq!(sample.state, State::Cancelled);
    assert_eq!(sample.failures, 0);
    assert_eq!(sample.max_buffered_batches, Some(0));
}
