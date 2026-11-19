//! Worker loop: pick a free-source task, CAS-claim it, execute, release the
//! per-source slot and cancel token.

use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use persistence_sqlite::repositories::source_capability_repo::SourceCapabilityRepo;

use super::events;
use super::state::{condvar, lock};
use super::QueuedTask;

pub(super) fn worker_loop() {
    loop {
        let (task, token) = {
            let mut guard = lock();
            let picked = loop {
                let position = guard
                    .queue
                    .iter()
                    .position(|candidate| !guard.busy_sources.contains(&candidate.source_key()));
                match position {
                    Some(index) => {
                        let task = guard.queue.remove(index).expect("position valid");
                        guard.busy_sources.insert(task.source_key());
                        let token = Arc::new(AtomicBool::new(false));
                        guard.active_tokens.insert(task.key(), token.clone());
                        break (task, token);
                    }
                    None => {
                        guard = condvar().wait(guard).expect("scheduler condvar poisoned");
                    }
                }
            };
            picked
        };
        run_task(&task, &token);
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
            events::write_row(task, "failed", Some("capability claim failed"), Some("DB"));
            return;
        }
    }
    events::write_row(task, "running", None, None);
    super::dispatch::run(task, &conn, token);
}
