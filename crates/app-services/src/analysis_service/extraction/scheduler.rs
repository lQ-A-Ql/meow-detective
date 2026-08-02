use crate::analysis_service::cancellation::ensure_not_cancelled;
use crate::analysis_service::error::AnalysisServiceError;
use crossbeam_channel::{bounded, unbounded, Receiver, Sender, TryRecvError};
use std::collections::BTreeMap;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard, TryLockError};
use std::time::{Duration, Instant};

const EXTRACTION_GATE_POLL_INTERVAL: Duration = Duration::from_millis(100);
const EXTRACTION_GATE_REPORT_INTERVAL: Duration = Duration::from_secs(1);
const DEFAULT_MAX_IN_FLIGHT_BYTES: usize = 256 * 1024 * 1024;
const THROTTLED_MAX_IN_FLIGHT_BYTES: usize = 128 * 1024 * 1024;

static EXTRACTION_GATE: ExtractionGate = ExtractionGate::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ExtractionSchedulingPolicy {
    pub(super) worker_count: usize,
    pub(super) max_in_flight_items: usize,
    pub(super) max_in_flight_bytes: usize,
}

impl ExtractionSchedulingPolicy {
    pub(super) fn for_current_process() -> Self {
        Self::for_runtime(
            crate::import_analysis::resolve_analysis_worker_count(None),
            crate::runtime_resources::current_rss_mb(),
            crate::runtime_resources::default_memory_soft_limit_mb(),
        )
    }

    fn for_runtime(worker_count: usize, rss_mb: u64, memory_soft_limit_mb: u64) -> Self {
        let worker_count = crate::import_scheduler::resolve_analysis_worker_count_for_memory(
            Some(worker_count),
            rss_mb,
            memory_soft_limit_mb,
        );
        let memory_throttled = rss_mb > 0 && rss_mb >= memory_soft_limit_mb;
        Self {
            worker_count,
            max_in_flight_items: worker_count,
            max_in_flight_bytes: if memory_throttled {
                THROTTLED_MAX_IN_FLIGHT_BYTES
            } else {
                DEFAULT_MAX_IN_FLIGHT_BYTES
            },
        }
    }
}

pub(super) enum PreparedWork<T, R> {
    Ready(R),
    Parallel { input: T, weight_bytes: usize },
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct SchedulerSnapshot {
    pub(super) submitted: usize,
    pub(super) completed: usize,
    pub(super) in_flight_items: usize,
    pub(super) in_flight_bytes: usize,
}

pub(super) fn acquire_extraction_slot(
    cancel_token: &AtomicBool,
    on_wait: impl FnMut(Duration),
) -> Result<MutexGuard<'static, ()>, AnalysisServiceError> {
    EXTRACTION_GATE.acquire(cancel_token, on_wait)
}

// Keep the independent scheduler hooks explicit: estimation, preparation,
// execution, ordered application, panic conversion, and observability.
#[allow(clippy::too_many_arguments)]
pub(super) fn run_bounded_ordered<I, T, R, E, C>(
    items: I,
    policy: ExtractionSchedulingPolicy,
    coordinator: &mut C,
    estimate_weight: impl Fn(&I::Item) -> usize,
    mut prepare: impl FnMut(&mut C, I::Item) -> Result<PreparedWork<T, R>, E>,
    work: impl Fn(T) -> R + Sync,
    mut apply: impl FnMut(&mut C, R) -> Result<(), E>,
    panic_error: impl Fn(usize, String) -> E + Sync,
    mut observe: impl FnMut(SchedulerSnapshot),
) -> Result<(), E>
where
    I: IntoIterator,
    T: Send,
    R: Send,
{
    let worker_count = policy.worker_count.max(1);
    let queue_bound = policy.max_in_flight_items.max(1);
    let stop = AtomicBool::new(false);

    std::thread::scope(|scope| {
        let (task_tx, task_rx) = bounded::<WorkerTask<T>>(queue_bound);
        let (result_tx, result_rx) = unbounded::<WorkerResult<R>>();
        let mut workers = Vec::with_capacity(worker_count);
        for worker_id in 0..worker_count {
            let receiver = task_rx.clone();
            let sender = result_tx.clone();
            let work_ref = &work;
            let stop_ref = &stop;
            workers.push(
                scope.spawn(move || worker_loop(worker_id, receiver, sender, stop_ref, work_ref)),
            );
        }
        drop(task_rx);
        drop(result_tx);

        let result = coordinate_pipeline(
            items,
            policy,
            coordinator,
            &estimate_weight,
            &task_tx,
            &result_rx,
            &mut prepare,
            &mut apply,
            &panic_error,
            &mut observe,
        );
        stop.store(true, Ordering::Release);
        drop(task_tx);

        let mut worker_panic = None;
        for (worker_id, worker) in workers.into_iter().enumerate() {
            if worker.join().is_err() && worker_panic.is_none() {
                worker_panic = Some(panic_error(
                    worker_id,
                    "analysis scheduler worker terminated unexpectedly".to_string(),
                ));
            }
        }
        match result {
            Err(error) => Err(error),
            Ok(()) => match worker_panic {
                Some(error) => Err(error),
                None => Ok(()),
            },
        }
    })
}

#[allow(clippy::too_many_arguments)]
fn coordinate_pipeline<I, T, R, E, C>(
    items: I,
    policy: ExtractionSchedulingPolicy,
    coordinator: &mut C,
    estimate_weight: &impl Fn(&I::Item) -> usize,
    task_tx: &Sender<WorkerTask<T>>,
    result_rx: &Receiver<WorkerResult<R>>,
    prepare: &mut impl FnMut(&mut C, I::Item) -> Result<PreparedWork<T, R>, E>,
    apply: &mut impl FnMut(&mut C, R) -> Result<(), E>,
    panic_error: &impl Fn(usize, String) -> E,
    observe: &mut impl FnMut(SchedulerSnapshot),
) -> Result<(), E>
where
    I: IntoIterator,
    T: Send,
    R: Send,
{
    let mut state = CoordinatorState::default();
    for item in items {
        make_capacity(
            &mut state,
            policy,
            estimate_weight(&item),
            result_rx,
            coordinator,
            apply,
            panic_error,
        )?;
        let prepared = prepare(coordinator, item)?;
        let sequence = state.submitted;
        state.submitted = state.submitted.saturating_add(1);
        match prepared {
            PreparedWork::Ready(result) => {
                state.pending.insert(sequence, result);
                apply_ready(&mut state, coordinator, apply)?;
            }
            PreparedWork::Parallel {
                input,
                weight_bytes,
            } => {
                make_capacity(
                    &mut state,
                    policy,
                    weight_bytes,
                    result_rx,
                    coordinator,
                    apply,
                    panic_error,
                )?;
                task_tx
                    .send(WorkerTask {
                        sequence,
                        input,
                        weight_bytes,
                    })
                    .map_err(|error| {
                        panic_error(
                            sequence,
                            format!("analysis scheduler queue closed: {error}"),
                        )
                    })?;
                state.in_flight_items = state.in_flight_items.saturating_add(1);
                state.in_flight_bytes = state.in_flight_bytes.saturating_add(weight_bytes);
                drain_completed(&mut state, result_rx, coordinator, apply, panic_error)?;
            }
        }
        observe(state.snapshot());
    }

    while state.in_flight_items > 0 {
        receive_completed(&mut state, result_rx, coordinator, apply, panic_error)?;
        observe(state.snapshot());
    }
    apply_ready(&mut state, coordinator, apply)?;
    Ok(())
}

fn make_capacity<R, E, C>(
    state: &mut CoordinatorState<R>,
    policy: ExtractionSchedulingPolicy,
    next_weight: usize,
    result_rx: &Receiver<WorkerResult<R>>,
    coordinator: &mut C,
    apply: &mut impl FnMut(&mut C, R) -> Result<(), E>,
    panic_error: &impl Fn(usize, String) -> E,
) -> Result<(), E> {
    while state.in_flight_items > 0
        && (state.in_flight_items >= policy.max_in_flight_items.max(1)
            || state.in_flight_bytes.saturating_add(next_weight)
                > policy.max_in_flight_bytes.max(1))
    {
        receive_completed(state, result_rx, coordinator, apply, panic_error)?;
    }
    Ok(())
}

fn drain_completed<R, E, C>(
    state: &mut CoordinatorState<R>,
    result_rx: &Receiver<WorkerResult<R>>,
    coordinator: &mut C,
    apply: &mut impl FnMut(&mut C, R) -> Result<(), E>,
    panic_error: &impl Fn(usize, String) -> E,
) -> Result<(), E> {
    loop {
        match result_rx.try_recv() {
            Ok(result) => record_completed(state, result, coordinator, apply, panic_error)?,
            Err(TryRecvError::Empty) => return Ok(()),
            Err(TryRecvError::Disconnected) => {
                return Err(panic_error(
                    state.next_apply,
                    "analysis scheduler result channel disconnected".to_string(),
                ));
            }
        }
    }
}

fn receive_completed<R, E, C>(
    state: &mut CoordinatorState<R>,
    result_rx: &Receiver<WorkerResult<R>>,
    coordinator: &mut C,
    apply: &mut impl FnMut(&mut C, R) -> Result<(), E>,
    panic_error: &impl Fn(usize, String) -> E,
) -> Result<(), E> {
    let result = result_rx.recv().map_err(|error| {
        panic_error(
            state.next_apply,
            format!("analysis scheduler result channel closed: {error}"),
        )
    })?;
    record_completed(state, result, coordinator, apply, panic_error)
}

fn record_completed<R, E, C>(
    state: &mut CoordinatorState<R>,
    result: WorkerResult<R>,
    coordinator: &mut C,
    apply: &mut impl FnMut(&mut C, R) -> Result<(), E>,
    panic_error: &impl Fn(usize, String) -> E,
) -> Result<(), E> {
    let (sequence, weight_bytes, output) = match result {
        WorkerResult::Completed {
            sequence,
            output,
            weight_bytes,
        } => (sequence, weight_bytes, output),
        WorkerResult::Panicked {
            sequence,
            message,
            weight_bytes,
        } => {
            state.release(weight_bytes);
            return Err(panic_error(sequence, message));
        }
    };
    state.release(weight_bytes);
    state.pending.insert(sequence, output);
    apply_ready(state, coordinator, apply)
}

fn apply_ready<R, E, C>(
    state: &mut CoordinatorState<R>,
    coordinator: &mut C,
    apply: &mut impl FnMut(&mut C, R) -> Result<(), E>,
) -> Result<(), E> {
    while let Some(result) = state.pending.remove(&state.next_apply) {
        apply(coordinator, result)?;
        state.completed = state.completed.saturating_add(1);
        state.next_apply = state.next_apply.saturating_add(1);
    }
    Ok(())
}

fn worker_loop<T, R>(
    _worker_id: usize,
    task_rx: Receiver<WorkerTask<T>>,
    result_tx: Sender<WorkerResult<R>>,
    stop: &AtomicBool,
    work: &(impl Fn(T) -> R + Sync),
) where
    T: Send,
    R: Send,
{
    while !stop.load(Ordering::Acquire) {
        let Ok(task) = task_rx.recv() else {
            break;
        };
        let WorkerTask {
            sequence,
            input,
            weight_bytes,
        } = task;
        let result = match catch_unwind(AssertUnwindSafe(|| work(input))) {
            Ok(output) => WorkerResult::Completed {
                sequence,
                output,
                weight_bytes,
            },
            Err(payload) => WorkerResult::Panicked {
                sequence,
                message: panic_message(payload),
                weight_bytes,
            },
        };
        if result_tx.send(result).is_err() {
            break;
        }
    }
}

struct WorkerTask<T> {
    sequence: usize,
    input: T,
    weight_bytes: usize,
}

enum WorkerResult<R> {
    Completed {
        sequence: usize,
        output: R,
        weight_bytes: usize,
    },
    Panicked {
        sequence: usize,
        message: String,
        weight_bytes: usize,
    },
}

struct CoordinatorState<R> {
    submitted: usize,
    completed: usize,
    next_apply: usize,
    in_flight_items: usize,
    in_flight_bytes: usize,
    pending: BTreeMap<usize, R>,
}

impl<R> Default for CoordinatorState<R> {
    fn default() -> Self {
        Self {
            submitted: 0,
            completed: 0,
            next_apply: 0,
            in_flight_items: 0,
            in_flight_bytes: 0,
            pending: BTreeMap::new(),
        }
    }
}

impl<R> CoordinatorState<R> {
    fn release(&mut self, weight_bytes: usize) {
        self.in_flight_items = self.in_flight_items.saturating_sub(1);
        self.in_flight_bytes = self.in_flight_bytes.saturating_sub(weight_bytes);
    }

    fn snapshot(&self) -> SchedulerSnapshot {
        SchedulerSnapshot {
            submitted: self.submitted,
            completed: self.completed,
            in_flight_items: self.in_flight_items,
            in_flight_bytes: self.in_flight_bytes,
        }
    }
}

struct ExtractionGate {
    mutex: Mutex<()>,
}

impl ExtractionGate {
    const fn new() -> Self {
        Self {
            mutex: Mutex::new(()),
        }
    }

    fn acquire<'a>(
        &'a self,
        cancel_token: &AtomicBool,
        mut on_wait: impl FnMut(Duration),
    ) -> Result<MutexGuard<'a, ()>, AnalysisServiceError> {
        let started = Instant::now();
        let mut last_report = None::<Instant>;
        loop {
            ensure_not_cancelled(cancel_token)?;
            match self.mutex.try_lock() {
                Ok(guard) => return Ok(guard),
                Err(TryLockError::Poisoned(poisoned)) => {
                    tracing::warn!("Recovering poisoned analysis extraction serial gate");
                    return Ok(poisoned.into_inner());
                }
                Err(TryLockError::WouldBlock) => {
                    if last_report
                        .is_none_or(|last| last.elapsed() >= EXTRACTION_GATE_REPORT_INTERVAL)
                    {
                        on_wait(started.elapsed());
                        last_report = Some(Instant::now());
                    }
                    std::thread::sleep(EXTRACTION_GATE_POLL_INTERVAL);
                }
            }
        }
    }
}

fn panic_message(payload: Box<dyn std::any::Any + Send>) -> String {
    if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_string()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        "analysis parser panicked without a string payload".to_string()
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/analysis_service/extraction/scheduler.rs"]
mod tests;
