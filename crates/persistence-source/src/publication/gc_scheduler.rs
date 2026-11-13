//! Coalesced requests and bounded work for retired analysis generations.

use super::analysis_gc::reclaim_analysis_generations;
use persistence_sqlite::{DbError, DbResult};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant};

const RETRY_DELAY: Duration = Duration::from_millis(250);

struct Request {
    revision: u64,
    due: Instant,
    running: bool,
    last_error: Option<String>,
}

#[derive(Default)]
struct State {
    requests: BTreeMap<PathBuf, Request>,
    revision: u64,
}

struct Scheduler {
    state: Mutex<State>,
    changed: Condvar,
    stopped: AtomicBool,
}

static SCHEDULER: OnceLock<Result<Arc<Scheduler>, String>> = OnceLock::new();

pub fn request_analysis_generation_gc(source_directory: &Path) -> DbResult<()> {
    if SCHEDULER.get().is_some_and(|result| {
        result
            .as_ref()
            .is_ok_and(|scheduler| scheduler.stopped.load(Ordering::Acquire))
    }) {
        return Err(DbError::System("generation GC worker has stopped".into()));
    }
    let source_directory = validated_source_directory(source_directory)?;
    let scheduler = scheduler()?;
    if scheduler.stopped.load(Ordering::Acquire) {
        return Err(DbError::System("generation GC worker has stopped".into()));
    }
    let mut state = scheduler.state.lock().map_err(lock_error)?;
    if scheduler.stopped.load(Ordering::Acquire) {
        return Err(DbError::System("generation GC worker has stopped".into()));
    }
    enqueue_request(&mut state, source_directory);
    scheduler.changed.notify_all();
    Ok(())
}

fn enqueue_request(state: &mut State, source_directory: PathBuf) {
    state.revision = state.revision.wrapping_add(1);
    let revision = state.revision;
    state
        .requests
        .entry(source_directory)
        .and_modify(|request| {
            request.revision = revision;
            request.due = Instant::now();
        })
        .or_insert(Request {
            revision,
            due: Instant::now(),
            running: false,
            last_error: None,
        });
}

fn validated_source_directory(path: &Path) -> DbResult<PathBuf> {
    let directory = std::fs::canonicalize(path)?;
    let source = directory
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| DbError::System("source directory has no valid identity".into()))?;
    let sources = directory
        .parent()
        .filter(|parent| parent.file_name().is_some_and(|name| name == "sources"))
        .ok_or_else(|| {
            DbError::System("source directory is outside the case sources layout".into())
        })?;
    let case_root = sources
        .parent()
        .ok_or_else(|| DbError::System("source directory has no case root".into()))?;
    crate::control::current_source_generation_root(
        case_root,
        &domain::DataSourceId(source.into()),
    )?;
    Ok(directory)
}

pub(crate) fn notify_generation_source_idle(source_directory: &Path) {
    let Some(Ok(scheduler)) = SCHEDULER.get() else {
        return;
    };
    let Ok(mut state) = scheduler.state.lock() else {
        tracing::error!("generation GC scheduler lock is poisoned");
        return;
    };
    if let Some(request) = state.requests.get_mut(source_directory) {
        request.due = Instant::now();
        scheduler.changed.notify_all();
    }
}

pub fn drain_analysis_generation_gc(timeout: Duration) -> DbResult<()> {
    let Some(result) = SCHEDULER.get() else {
        return Ok(());
    };
    let scheduler = result
        .as_ref()
        .map_err(|error| DbError::System(error.clone()))?;
    let deadline = Instant::now() + timeout;
    let mut state = scheduler.state.lock().map_err(lock_error)?;
    while !state.requests.is_empty() {
        if scheduler.stopped.load(Ordering::Acquire) {
            return Err(DbError::System(
                "generation GC worker stopped unexpectedly".into(),
            ));
        }
        let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
            let error = state
                .requests
                .values()
                .find_map(|request| request.last_error.as_deref());
            return Err(DbError::System(format!(
                "generation GC drain timed out with {} pending sources; last error: {}",
                state.requests.len(),
                error.unwrap_or("none")
            )));
        };
        state = scheduler
            .changed
            .wait_timeout(state, remaining)
            .map_err(lock_error)?
            .0;
    }
    Ok(())
}

fn scheduler() -> DbResult<Arc<Scheduler>> {
    SCHEDULER
        .get_or_init(|| {
            let scheduler = Arc::new(Scheduler {
                state: Mutex::new(State::default()),
                changed: Condvar::new(),
                stopped: AtomicBool::new(false),
            });
            let worker = Arc::clone(&scheduler);
            std::thread::Builder::new()
                .name("source-generation-gc".into())
                .spawn(move || {
                    let _exit = WorkerExit(&worker);
                    if let Err(error) = run_worker(&worker) {
                        tracing::error!(%error, "generation GC worker failed");
                    }
                })
                .map_err(|error| error.to_string())?;
            Ok(scheduler)
        })
        .as_ref()
        .cloned()
        .map_err(|error| DbError::System(error.clone()))
}

struct WorkerExit<'a>(&'a Scheduler);

impl Drop for WorkerExit<'_> {
    fn drop(&mut self) {
        self.0.stopped.store(true, Ordering::Release);
        self.0.changed.notify_all();
    }
}

fn run_worker(scheduler: &Scheduler) -> DbResult<()> {
    loop {
        let (path, revision) = next_request(scheduler)?;
        let result = reclaim_analysis_generations(&path);
        let mut state = scheduler.state.lock().map_err(lock_error)?;
        let Some(request) = state.requests.get_mut(&path) else {
            continue;
        };
        request.running = false;
        match result {
            Ok(Some(outcome)) if !outcome.pending && request.revision == revision => {
                state.requests.remove(&path);
            }
            Ok(_) => {
                request.due = Instant::now() + RETRY_DELAY;
                request.last_error = None;
            }
            Err(error) => {
                let error = error.to_string();
                if request.last_error.as_ref() != Some(&error) {
                    tracing::warn!(%error, "generation GC request retained for retry");
                }
                request.last_error = Some(error);
                request.due = Instant::now() + RETRY_DELAY;
            }
        }
        scheduler.changed.notify_all();
    }
}

fn next_request(scheduler: &Scheduler) -> DbResult<(PathBuf, u64)> {
    let mut state = scheduler.state.lock().map_err(lock_error)?;
    loop {
        let next = state
            .requests
            .iter()
            .filter(|(_, request)| !request.running)
            .min_by_key(|(_, request)| request.due)
            .map(|(path, request)| (path.clone(), request.due));
        match next {
            Some((path, due)) if due <= Instant::now() => {
                let request = state
                    .requests
                    .get_mut(&path)
                    .expect("queued GC request exists");
                request.running = true;
                return Ok((path, request.revision));
            }
            Some((_, due)) => {
                state = scheduler
                    .changed
                    .wait_timeout(state, due.saturating_duration_since(Instant::now()))
                    .map_err(lock_error)?
                    .0;
            }
            None => state = scheduler.changed.wait(state).map_err(lock_error)?,
        }
    }
}

fn lock_error<T>(_: std::sync::PoisonError<T>) -> DbError {
    DbError::System("generation GC scheduler lock is poisoned".into())
}

#[cfg(test)]
#[path = "../../tests/unit/publication/gc_scheduler.rs"]
mod tests;
