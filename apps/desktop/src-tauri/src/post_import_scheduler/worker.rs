//! Worker loop: pick a free-source task, wait for an active import to yield,
//! CAS-claim it, execute, release the per-source slot, memory guard and
//! cancel token.

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, MutexGuard};
use std::time::{Duration, Instant};

use app_services::import_scheduler::global_import_admission;
use persistence_sqlite::repositories::source_capability_repo::SourceCapabilityRepo;

use super::events;
use super::resource::{
    release_degraded_slot, try_take_degraded_slot, PostMemoryGuard,
    POST_IMPORT_IMPORT_PRIORITY_WAIT,
};
use super::state::{condvar, lock, SchedulerState};
use super::QueuedTask;

/// Outcome of the import-priority gate.
enum GateOutcome {
    /// Imports drained (or grace expired without a degraded slot).
    Normal,
    /// Grace expired; this worker holds the single degraded slot.
    DegradedSlot,
}

/// Import-priority gate: before claiming a NEW task, yield to an active import.
/// Poll every 500ms so a cancel_case that empties the queue can wake the wait.
/// After the bounded grace, exactly one worker may proceed via the degraded
/// slot so queued work cannot starve forever. Tasks already running when the
/// import starts are never preempted; this gate only affects new claims.
fn import_priority_gate(
    guard: MutexGuard<'static, SchedulerState>,
) -> (MutexGuard<'static, SchedulerState>, GateOutcome) {
    let deadline = Instant::now() + POST_IMPORT_IMPORT_PRIORITY_WAIT;
    let mut guard = guard;
    loop {
        if global_import_admission().active_weight() == 0 {
            return (guard, GateOutcome::Normal);
        }
        if Instant::now() >= deadline && try_take_degraded_slot() {
            return (guard, GateOutcome::DegradedSlot);
        }
        let (recovered, _timed_out) = condvar()
            .wait_timeout(guard, Duration::from_millis(500))
            .unwrap_or_else(|error| error.into_inner());
        guard = recovered;
        // Cancelled/drained while waiting: leave the gate; the normal pick loop
        // falls back to a condvar wait without holding the degraded slot.
        if guard.queue.is_empty() {
            return (guard, GateOutcome::Normal);
        }
    }
}

pub(super) fn worker_loop() {
    loop {
        let (task, token, degraded) = {
            let mut guard = lock();
            let picked = loop {
                let position = guard
                    .queue
                    .iter()
                    .position(|candidate| !guard.busy_sources.contains(&candidate.source_key()));
                match position {
                    Some(_) => {
                        let (recovered, outcome) = import_priority_gate(guard);
                        guard = recovered;
                        // Re-resolve after the wait: queue and busy set may have
                        // changed; the gate may have held a degraded slot.
                        let position = guard.queue.iter().position(|candidate| {
                            !guard.busy_sources.contains(&candidate.source_key())
                        });
                        match position {
                            Some(index) => {
                                let task = guard.queue.remove(index).expect("position valid");
                                guard.busy_sources.insert(task.source_key());
                                let token = Arc::new(AtomicBool::new(false));
                                guard.active_tokens.insert(task.key(), token.clone());
                                break (task, token, matches!(outcome, GateOutcome::DegradedSlot));
                            }
                            None => {
                                if matches!(outcome, GateOutcome::DegradedSlot) {
                                    release_degraded_slot();
                                }
                                continue;
                            }
                        }
                    }
                    None => {
                        guard = condvar().wait(guard).expect("scheduler condvar poisoned");
                    }
                }
            };
            picked
        };
        let _memory = PostMemoryGuard::acquire();
        run_task(&task, &token);
        if degraded {
            release_degraded_slot();
        }
        {
            let mut guard = lock();
            guard.busy_sources.remove(&task.source_key());
            guard.active_tokens.remove(&task.key());
        }
        condvar().notify_all();
    }
}

fn active_case_matches(task: &QueuedTask) -> bool {
    let guard = match task.app_state.active_case.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    guard
        .as_ref()
        .map(|active| active.meta.id.0 == task.case_id)
        .unwrap_or(false)
}

fn run_task(task: &QueuedTask, token: &Arc<AtomicBool>) {
    if !active_case_matches(task) {
        tracing::info!(
            case_id = %task.case_id,
            source_id = %task.source_id,
            capability = task.capability.wire(),
            "post-import task dropped: active case changed or closed"
        );
        return;
    }
    let db_path = task.case_root.join("app.db");
    let conn = match app_services::connection::open_existing_case_db(&db_path) {
        Ok(conn) => conn,
        Err(error) => {
            tracing::warn!(error = %error, "post-import task: cannot open case DB");
            events::write_row(
                task,
                "failed",
                Some("case database unavailable"),
                Some("IO"),
                Some("done"),
            );
            return;
        }
    };
    // CAS claim: execute only when we own the queued/pending row. A lost race
    // (another worker owns it, row already finished) leaves the row untouched.
    let claimed = SourceCapabilityRepo::new(&conn).try_claim(
        &task.case_id,
        &task.source_id,
        task.capability.wire(),
    );
    match claimed {
        Ok(true) => {}
        Ok(false) => {
            tracing::info!(
                case_id = %task.case_id,
                source_id = %task.source_id,
                capability = task.capability.wire(),
                "post-import task skipped: claim lost"
            );
            return;
        }
        Err(error) => {
            tracing::warn!(error = %error, "post-import task: claim query failed");
            events::write_row(
                task,
                "failed",
                Some("capability claim failed"),
                Some("DB"),
                Some("done"),
            );
            return;
        }
    }
    events::write_row(task, "running", None, None, Some("running"));
    // Manual-acceptance injection hook: MEOW_POST_IMPORT_DELAY_MS (default 0)
    // pauses before execution to observe a slow timeline; no-op in production.
    super::injection::apply_injection_delay();
    super::dispatch::run(task, &conn, token);
}
