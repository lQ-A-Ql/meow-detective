//! Minimal single-worker post-import capability scheduler (L1).
//!
//! After a catalog-only import job completes, `timeline` and `artifacts` rows
//! marked `queued` are executed one at a time on a dedicated worker thread.
//! L2 hardens queue priority, publication leases, fencing and restart
//! recovery; this module intentionally stays small: one worker, a
//! Mutex/Condvar queue, active-case fencing before dispatch, and task-confirmed
//! row writes with status/progress events.

mod dispatch;
mod enqueue;
mod events;

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{Condvar, Mutex, OnceLock};

use tauri::AppHandle;

use crate::state::AppState;

pub(crate) use enqueue::enqueue_for_source;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum QueuedCapability {
    Timeline,
    Artifacts,
}

impl QueuedCapability {
    pub(super) fn wire(self) -> &'static str {
        match self {
            Self::Timeline => "timeline",
            Self::Artifacts => "artifacts",
        }
    }
}

pub(super) struct QueuedTask {
    pub(super) app_state: AppState,
    pub(super) app: Option<AppHandle>,
    pub(super) case_id: String,
    pub(super) case_root: PathBuf,
    pub(super) source_id: String,
    pub(super) capability: QueuedCapability,
}

struct SchedulerState {
    queue: VecDeque<QueuedTask>,
    worker_started: bool,
}

fn state() -> &'static Mutex<SchedulerState> {
    static STATE: OnceLock<Mutex<SchedulerState>> = OnceLock::new();
    STATE.get_or_init(|| {
        Mutex::new(SchedulerState {
            queue: VecDeque::new(),
            worker_started: false,
        })
    })
}

fn condvar() -> &'static Condvar {
    static CV: OnceLock<Condvar> = OnceLock::new();
    CV.get_or_init(Condvar::new)
}

fn worker_loop() {
    loop {
        let task = {
            let mut guard = match state().lock() {
                Ok(guard) => guard,
                Err(poisoned) => poisoned.into_inner(),
            };
            while guard.queue.is_empty() {
                guard = match condvar().wait(guard) {
                    Ok(guard) => guard,
                    Err(poisoned) => poisoned.into_inner(),
                };
            }
            guard.queue.pop_front()
        };
        let Some(task) = task else { continue };
        run_task(&task);
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

fn run_task(task: &QueuedTask) {
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
    events::write_row(task, "running", None, None);
    match task.capability {
        QueuedCapability::Timeline => dispatch::run_timeline(task, &conn),
        QueuedCapability::Artifacts => dispatch::run_artifacts(task, &conn),
    }
}
