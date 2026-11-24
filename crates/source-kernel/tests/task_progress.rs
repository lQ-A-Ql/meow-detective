use source_kernel::task_runtime::progress::TaskProgressReporter;
use std::{thread, time::Duration};
use transport::dto::{
    TaskProgressDto, TaskProgressModeDto as Mode, TaskProgressStateDto as State,
    TaskProgressUnitDto as Unit, TaskStageKeyDto as Stage,
};

#[test]
fn weighted_progress_counts_only_completed_stages_and_current_work() {
    let mut reporter = TaskProgressReporter::new("job", "case", false);
    reporter.start(Stage::Prepare);
    assert_eq!(reporter.heartbeat(false).progress, 0);
    assert_eq!(reporter.finish_stage(Stage::Prepare, false).progress, 8);
    reporter.start(Stage::Enumerate);
    reporter.advance(50, Some(100), Unit::Items);
    let event = reporter.heartbeat(false);
    assert_eq!(event.progress, 26);
    assert_eq!(event.mode, Mode::Estimated);
    assert_eq!(event.completed_units, 50);
    reporter.advance(10, Some(100), Unit::Items);
    assert_eq!(reporter.heartbeat(false).progress, 26);
    assert_eq!(reporter.finish_stage(Stage::Enumerate, false).progress, 45);
    reporter.start(Stage::Analyze);
    assert!(
        reporter
            .heartbeat(false)
            .stages
            .iter()
            .filter(|stage| stage.state == State::Running)
            .count()
            <= 1
    );
    assert_eq!(reporter.heartbeat(false).mode, Mode::Indeterminate);
    let cancelled = reporter.finish(State::Cancelled);
    assert_eq!(cancelled.progress, 45);
    assert_eq!(cancelled.state, State::Cancelled);
    assert!(cancelled.eta_seconds.is_none());
}

#[test]
fn heartbeat_changes_sequence_without_advancing_work_and_dto_round_trips() {
    let mut reporter = TaskProgressReporter::new("job", "case", true);
    let first = reporter.heartbeat(false);
    thread::sleep(Duration::from_millis(10));
    let second = reporter.heartbeat(true);
    assert!(second.seq > first.seq);
    assert!(second.heartbeat_at > first.heartbeat_at);
    assert_eq!(second.completed_units, first.completed_units);
    assert_eq!(second.progress, first.progress);
    assert_eq!(
        second.stages.iter().map(|stage| stage.weight).sum::<u32>(),
        100
    );
    let encoded = serde_json::to_value(&second).unwrap();
    let decoded: TaskProgressDto = serde_json::from_value(encoded).unwrap();
    assert_eq!(decoded.seq, second.seq);
    assert_eq!(decoded.state, State::Cancelling);
    assert_eq!(reporter.finish(State::Completed).progress, 100);
}

#[test]
fn eta_requires_real_elapsed_work_samples_and_resets_at_the_next_stage() {
    let mut reporter = TaskProgressReporter::new("job", "case", false);
    reporter.start(Stage::Enumerate);
    reporter.advance(0, Some(100), Unit::Items);
    assert!(reporter.heartbeat(false).eta_seconds.is_none());
    for completed in [10, 20, 30] {
        thread::sleep(Duration::from_secs(2));
        reporter.advance(completed, Some(100), Unit::Items);
    }
    let estimated = reporter.heartbeat(false);
    assert!(estimated.rate.is_some_and(|rate| rate > 0.0));
    assert!(estimated.eta_seconds.is_some_and(|remaining| remaining > 0));
    reporter.advance(40, None, Unit::Items);
    assert!(reporter.heartbeat(false).eta_seconds.is_none());
    reporter.start(Stage::Analyze);
    let next = reporter.heartbeat(false);
    assert!(next.rate.is_none());
    assert!(next.eta_seconds.is_none());
}

#[test]
fn independent_timeline_and_artifact_plans_have_weighted_work_and_publication() {
    for analysis in [false, true] {
        let mut reporter = TaskProgressReporter::for_capability("job", "case", "source", analysis);
        let event = reporter.finish_stage(Stage::Prepare, false);
        assert_eq!(
            event.stages.iter().map(|stage| stage.weight).sum::<u32>(),
            100
        );
        assert_eq!(event.data_source_id.as_deref(), Some("source"));
        reporter.start(if analysis {
            Stage::Analyze
        } else {
            Stage::Index
        });
        reporter.advance(50, Some(100), Unit::Items);
        let working = reporter.heartbeat(false);
        assert_eq!(working.progress, if analysis { 55 } else { 60 });
        reporter.start(Stage::Publish);
        reporter.start(if analysis {
            Stage::Analyze
        } else {
            Stage::Index
        });
        let resumed = reporter.heartbeat(false);
        assert_eq!(
            resumed
                .stages
                .iter()
                .find(|stage| stage.key == Stage::Publish)
                .unwrap()
                .state,
            State::Pending
        );
        let partial = reporter.finish(State::Partial);
        assert_eq!(partial.mode, Mode::Determinate);
        assert!(partial.progress < 100);
    }
}
