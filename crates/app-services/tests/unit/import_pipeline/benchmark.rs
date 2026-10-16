use super::*;
use crate::import_pipeline::profile::PhaseRecorder;

#[test]
fn timeline_write_parents_include_their_exclusive_children() {
    let profiler = ImportProfiler::default();
    for phase in [
        Phase::Timeline,
        Phase::TimelineEvents,
        Phase::TimelineGraph,
        Phase::TimelineEventInsert,
        Phase::TimelineGraphCommit,
    ] {
        profiler.record(
            None,
            phase,
            Some(Duration::from_secs(1)),
            State::Completed,
            Counters::default(),
        );
    }
    for sample in profiler.report().samples {
        assert_eq!(
            sample.inclusive,
            matches!(
                sample.phase,
                Phase::Timeline | Phase::TimelineEvents | Phase::TimelineGraph
            )
        );
    }
}

#[test]
fn partition_work_is_aggregated_without_double_counting_source_wall_time() {
    let profiler = ImportProfiler::default();
    profiler.record(
        None,
        Phase::Enumeration,
        Some(Duration::from_secs(2)),
        State::Completed,
        Counters::default(),
    );
    for partition in [1, 2] {
        let mut local = PhaseRecorder::new(Some(profiler.clone()), Some(partition));
        local.record_duration(
            Phase::MftRead,
            Some(Duration::from_secs(3)),
            State::Completed,
        );
        local.count(
            Phase::MftRead,
            Counters {
                scanned: 100,
                bytes: 1024 * 1024,
                ..Default::default()
            },
        );
    }
    let report = profiler.report();
    let total = &report.partition_aggregates[0];
    assert_eq!(total.elapsed_ns, Some(6_000_000_000));
    assert_eq!(total.scanned_records, 200);
    assert_eq!(total.records_per_sec, Some(200.0 / 6.0));
    assert_eq!(total.logical_mb_per_sec, Some(2.0 / 6.0));
    assert_eq!(report.samples.len(), 3);
    assert_eq!(report.samples[0].elapsed_ns, Some(2_000_000_000));
    assert!(report.samples[0].inclusive);
    assert!(total.physical_bytes_read.is_none());
}

#[test]
fn missing_and_zero_duration_are_not_successful_zero_throughput() {
    let profiler = ImportProfiler::default();
    profiler.record(
        None,
        Phase::SourceIndexBuild,
        None,
        State::NotApplicable,
        Counters::default(),
    );
    profiler.record(
        Some(1),
        Phase::MftRead,
        Some(Duration::ZERO),
        State::Completed,
        Counters {
            scanned: 100,
            ..Default::default()
        },
    );
    let report = profiler.report();
    assert_eq!(report.samples[0].state, State::NotApplicable);
    assert_eq!(report.samples[0].elapsed_ns, None);
    assert!(report
        .samples
        .iter()
        .all(|sample| sample.records_per_sec.is_none()));
}

#[test]
fn disabled_profiling_preserves_operation_and_error() {
    let mut recorder = PhaseRecorder::new(None, Some(1));
    let result = recorder.measure(Phase::MftRead, || Err::<(), _>("same-error"));
    assert_eq!(result, Err("same-error"));
    let profiler = ImportProfiler::default();
    assert!(profiler.report().samples.is_empty());
}

#[test]
fn failures_and_cancellation_are_retained_in_partition_aggregates() {
    let profiler = ImportProfiler::default();
    let mut recorder = PhaseRecorder::new(Some(profiler.clone()), Some(7));
    let error = recorder.measure(Phase::MftRead, || Err::<(), _>("synthetic-read-error"));
    assert!(error.is_err());
    let cancelled = recorder.measure(Phase::Enumeration, || Err::<(), _>("Cancelled"));
    assert!(cancelled.is_err());
    recorder.cancel(Phase::Enumeration);
    drop(recorder);
    let report = profiler.report();
    let read = report
        .samples
        .iter()
        .find(|sample| sample.phase == Phase::MftRead)
        .unwrap();
    assert_eq!(read.state, State::Failed);
    assert_eq!(read.failures, 1);
    assert_eq!(read.logical_bytes_read, 0);
    let enumeration = report
        .samples
        .iter()
        .find(|sample| sample.phase == Phase::Enumeration)
        .unwrap();
    assert_eq!(enumeration.state, State::Cancelled);
    assert_eq!(enumeration.failures, 0);
    assert!(!serde_json::to_string(&report)
        .unwrap()
        .contains("synthetic-read-error"));
}
