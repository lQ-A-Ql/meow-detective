//! Shared scheduler state: bounded queue, per-source busy set, live cancel
//! tokens, admission policy, and worker startup.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::sync::{Condvar, Mutex, OnceLock};

use super::resource::MAX_AUTO_TASKS_PER_CASE;
use super::QueuedTask;

/// Bounded worker count: derived from
/// `source_kernel::resource_budget::resolve_analysis_worker_count(None)`, whose
/// unrequested default caps CPU-bound analysis workers at 2
/// (`resolve_requested` returns `capacity.min(2)`).
pub(super) const WORKER_COUNT: usize = 2;

/// Per-case cap on outstanding P0 user-retry queue slots. Auto tasks merge by
/// `(source, capability)` and stay bounded without a separate cap; a retry
/// beyond this slot count returns a typed busy error instead of piling up.
const MAX_P0_RETRY_PER_CASE: usize = 4;

/// Admission outcome returned to the command layer.
pub(crate) enum Admission {
    Accepted,
    AlreadyQueued,
    Busy,
}

pub(super) struct SchedulerState {
    pub(super) queue: VecDeque<QueuedTask>,
    /// `case_id|source_id` currently executing; enforces per-source serialization.
    pub(super) busy_sources: HashSet<String>,
    /// `case_id|source_id|capability|revision` → live cancel token for running tasks.
    pub(super) active_tokens: HashMap<String, Arc<AtomicBool>>,
    pub(super) workers_started: usize,
}

pub(super) fn state() -> &'static Mutex<SchedulerState> {
    static STATE: OnceLock<Mutex<SchedulerState>> = OnceLock::new();
    STATE.get_or_init(|| {
        Mutex::new(SchedulerState {
            queue: VecDeque::new(),
            busy_sources: HashSet::new(),
            active_tokens: HashMap::new(),
            workers_started: 0,
        })
    })
}

pub(super) fn condvar() -> &'static Condvar {
    static CV: OnceLock<Condvar> = OnceLock::new();
    CV.get_or_init(Condvar::new)
}

pub(super) fn lock() -> std::sync::MutexGuard<'static, SchedulerState> {
    match state().lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

pub(super) fn ensure_workers(guard: &mut SchedulerState) {
    while guard.workers_started < WORKER_COUNT {
        guard.workers_started += 1;
        let index = guard.workers_started;
        std::thread::Builder::new()
            .name(format!("post-import-scheduler-{index}"))
            .spawn(super::worker::worker_loop)
            .expect("post-import scheduler thread must start");
    }
}

/// Shared admission routine: dedup by task key, P0 slot cap, priority insert.
pub(super) fn admit(task: QueuedTask) -> Admission {
    let mut guard = lock();
    let key = task.key();
    if guard.queue.iter().any(|existing| existing.key() == key)
        || guard.active_tokens.contains_key(&key)
    {
        return Admission::AlreadyQueued;
    }
    if task.priority == super::Priority::UserRetry {
        let p0_outstanding = guard
            .queue
            .iter()
            .filter(|existing| {
                existing.priority == super::Priority::UserRetry && existing.case_id == task.case_id
            })
            .count();
        if p0_outstanding >= MAX_P0_RETRY_PER_CASE {
            return Admission::Busy;
        }
    } else {
        // Auto work: natural per-case bound is MAX_AUTO_TASKS_PER_CASE
        // capabilities per source (timeline + artifacts), i.e. 2 × sources.
        // A source already holding that many auto queue slots drops the
        // duplicate silently (AlreadyQueued) instead of raising an error.
        let source_key = task.source_key();
        let auto_for_source = guard
            .queue
            .iter()
            .filter(|existing| {
                existing.priority == super::Priority::Auto && existing.source_key() == source_key
            })
            .count();
        if auto_for_source >= MAX_AUTO_TASKS_PER_CASE {
            return Admission::AlreadyQueued;
        }
    }
    match task.priority {
        super::Priority::UserRetry => {
            // Insert after every existing P0 entry; P1 auto tasks trail behind.
            let position = guard
                .queue
                .iter()
                .position(|existing| existing.priority != super::Priority::UserRetry)
                .unwrap_or(guard.queue.len());
            guard.queue.insert(position, task);
        }
        super::Priority::Auto => guard.queue.push_back(task),
    }
    ensure_workers(&mut guard);
    drop(guard);
    condvar().notify_all();
    Admission::Accepted
}
