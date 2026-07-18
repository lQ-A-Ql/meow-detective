use persistence_sqlite::repositories::processing_phase_repo::{
    ProcessingPhase, ProcessingPhaseState,
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
    let result = action();
    drop(heartbeat);
    match result {
        Ok(stats) => complete_phase(runner, &attempt, stats, report),
        Err(error) => fail_phase(runner, &attempt, &error, report),
    }
    report.phases[outcome_index].state
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
