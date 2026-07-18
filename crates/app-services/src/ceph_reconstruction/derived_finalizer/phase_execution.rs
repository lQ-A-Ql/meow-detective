use persistence_sqlite::repositories::processing_phase_repo::{
    ProcessingPhase, ProcessingPhaseState,
};
use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::Instant,
};

use super::{
    outcome::{DerivedFinalizationPhaseOutcome, DerivedFinalizationReport},
    phase_runner::{PhaseClaim, ProcessingPhaseAttempt, ProcessingPhaseRunner},
};

pub(super) fn run_phase(
    runner: &ProcessingPhaseRunner<'_>,
    phase: ProcessingPhase,
    report: &mut DerivedFinalizationReport,
    action: impl FnOnce() -> Result<String, String>,
) -> ProcessingPhaseState {
    run_phase_internal(runner, phase, report, None, action)
}

pub(super) fn run_cancellable_phase(
    runner: &ProcessingPhaseRunner<'_>,
    phase: ProcessingPhase,
    report: &mut DerivedFinalizationReport,
    cancel_token: &AtomicBool,
    action: impl FnOnce() -> Result<String, String>,
) -> ProcessingPhaseState {
    run_phase_internal(runner, phase, report, Some(cancel_token), action)
}

fn run_phase_internal(
    runner: &ProcessingPhaseRunner<'_>,
    phase: ProcessingPhase,
    report: &mut DerivedFinalizationReport,
    cancel_token: Option<&AtomicBool>,
    action: impl FnOnce() -> Result<String, String>,
) -> ProcessingPhaseState {
    let outcome_index = report.phases.len();
    let Some(attempt) = claim_phase(runner, phase, report) else {
        return report.phases[outcome_index].state;
    };
    let heartbeat = match runner.start_heartbeat(&attempt) {
        Ok(heartbeat) => heartbeat,
        Err(error) => {
            fail_phase(
                runner,
                &attempt,
                &format!("Start processing-phase heartbeat: {error}"),
                report,
            );
            return report.phases[outcome_index].state;
        }
    };
    let started = Instant::now();
    let result = action();
    let lease_lost = heartbeat.lease_lost();
    drop(heartbeat);
    match result {
        Ok(_stats) if lease_lost => fail_phase(
            runner,
            &attempt,
            "processing phase lease was lost before publication",
            report,
        ),
        Ok(stats) => complete_phase(runner, &attempt, stats, report),
        Err(error) if cancel_token.is_some_and(|token| token.load(Ordering::Relaxed)) => {
            defer_cancelled_phase(runner, &attempt, &error, report)
        }
        Err(error) => fail_phase(runner, &attempt, &error, report),
    }
    let state = report.phases[outcome_index].state;
    tracing::info!(
        data_source_id = runner.data_source_id(),
        phase = %phase,
        state = %state,
        elapsed_ms = started.elapsed().as_millis(),
        "Derived-source processing phase finished"
    );
    state
}

pub(super) fn claim_phase(
    runner: &ProcessingPhaseRunner<'_>,
    phase: ProcessingPhase,
    report: &mut DerivedFinalizationReport,
) -> Option<ProcessingPhaseAttempt> {
    match runner.claim(phase) {
        Ok(PhaseClaim::Acquired(attempt)) => Some(attempt),
        Ok(PhaseClaim::Ready(outcome) | PhaseClaim::Busy(outcome)) => {
            report.push(outcome);
            None
        }
        Err(error) => {
            push_storage_error(phase, error, report);
            None
        }
    }
}

pub(super) fn complete_phase(
    runner: &ProcessingPhaseRunner<'_>,
    attempt: &ProcessingPhaseAttempt,
    stats: String,
    report: &mut DerivedFinalizationReport,
) {
    match runner.ready(attempt, &stats) {
        Ok(outcome) => report.push(outcome),
        Err(error) => push_storage_error(attempt.phase(), error, report),
    }
}

pub(super) fn fail_phase(
    runner: &ProcessingPhaseRunner<'_>,
    attempt: &ProcessingPhaseAttempt,
    error: &str,
    report: &mut DerivedFinalizationReport,
) {
    match runner.failed(attempt, error) {
        Ok(outcome) => report.push(outcome),
        Err(state_error) => {
            tracing::warn!(
                phase = %attempt.phase(),
                error = %state_error,
                original_error = error,
                "Failed to persist a derived-source processing failure"
            );
            push_storage_error(attempt.phase(), state_error, report);
        }
    }
}

fn defer_cancelled_phase(
    runner: &ProcessingPhaseRunner<'_>,
    attempt: &ProcessingPhaseAttempt,
    detail: &str,
    report: &mut DerivedFinalizationReport,
) {
    match runner.deferred(
        attempt,
        r#"{"reason":"userCancelled","retryable":true}"#,
        detail,
    ) {
        Ok(outcome) => report.push(outcome),
        Err(state_error) => {
            tracing::warn!(
                phase = %attempt.phase(),
                error = %state_error,
                cancellation_detail = detail,
                "Failed to persist a cancelled derived-source processing phase"
            );
            push_storage_error(attempt.phase(), state_error, report);
        }
    }
}

pub(super) fn push_storage_error(
    phase: ProcessingPhase,
    error: persistence_sqlite::DbError,
    report: &mut DerivedFinalizationReport,
) {
    report.push(DerivedFinalizationPhaseOutcome {
        phase,
        state: ProcessingPhaseState::Failed,
        stats_json: "{}".to_string(),
        error: Some(error.to_string()),
    });
}
