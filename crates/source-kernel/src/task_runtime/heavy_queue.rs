//! Serialized execution queue for scoped heavy tasks.

use std::collections::{HashSet, VecDeque};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};

use super::registry::{TaskRegistry, TaskResult};
use super::{TaskPriority, TaskScope};

type HeavyTaskBody = Box<dyn FnOnce() -> TaskResult + Send + 'static>;

struct QueuedHeavyTask {
    task_id: String,
    cancel_token: Arc<AtomicBool>,
    body: HeavyTaskBody,
    source: Option<(String, String)>,
    priority: TaskPriority,
}

#[derive(Default)]
struct QueueState {
    tasks: VecDeque<QueuedHeavyTask>,
    busy_sources: HashSet<(String, String)>,
}

struct HeavyQueueState {
    queue: Mutex<QueueState>,
    changed: Condvar,
    shutdown: AtomicBool,
    registry: Arc<TaskRegistry>,
}

pub(super) struct HeavyTaskQueue {
    state: Arc<HeavyQueueState>,
}

impl HeavyTaskQueue {
    pub(super) fn new(registry: Arc<TaskRegistry>, name: &str, workers: usize) -> Self {
        let state = Arc::new(HeavyQueueState {
            queue: Mutex::new(QueueState::default()),
            changed: Condvar::new(),
            shutdown: AtomicBool::new(false),
            registry,
        });
        for index in 0..workers {
            let worker_state = Arc::clone(&state);
            std::thread::Builder::new()
                .name(format!("{name}-{index}"))
                .spawn(move || run_worker(worker_state))
                .expect("background queue worker must start");
        }
        Self { state }
    }

    pub(super) fn enqueue<F>(
        &self,
        task_id: String,
        scope: Option<&TaskScope>,
        priority: TaskPriority,
        cancel_token: Arc<AtomicBool>,
        body: F,
    ) -> Result<(), ()>
    where
        F: FnOnce() -> TaskResult + Send + 'static,
    {
        if self.state.shutdown.load(Ordering::Acquire) {
            return Err(());
        }
        let mut queue = self
            .state
            .queue
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if self.state.shutdown.load(Ordering::Acquire) {
            return Err(());
        }
        let task = QueuedHeavyTask {
            task_id,
            cancel_token,
            body: Box::new(body),
            source: scope.and_then(|scope| {
                scope
                    .data_source_id
                    .as_ref()
                    .map(|source| (scope.case_id.clone(), source.clone()))
            }),
            priority,
        };
        let position = queue
            .tasks
            .iter()
            .position(|queued| queued.priority < priority)
            .unwrap_or(queue.tasks.len());
        queue.tasks.insert(position, task);
        self.state.changed.notify_all();
        Ok(())
    }

    pub(super) fn cancel_queued(&self, task_ids: &[String]) {
        if task_ids.is_empty() {
            return;
        }
        let task_ids = task_ids.iter().collect::<HashSet<_>>();
        let removed = {
            let mut queue = self
                .state
                .queue
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            let mut removed = Vec::new();
            queue.tasks.retain(|task| {
                if task_ids.contains(&task.task_id) {
                    removed.push(task.task_id.clone());
                    false
                } else {
                    true
                }
            });
            removed
        };
        for task_id in removed {
            self.state.registry.complete(task_id, Ok(()));
        }
    }

    pub(super) fn shutdown(&self) {
        self.state.shutdown.store(true, Ordering::Release);
        self.state.changed.notify_all();
    }
}

fn run_worker(state: Arc<HeavyQueueState>) {
    loop {
        let Some(task) = next_task(&state) else {
            return;
        };
        let result = if task.cancel_token.load(Ordering::Acquire) {
            Ok(())
        } else {
            catch_unwind(AssertUnwindSafe(task.body))
                .unwrap_or_else(|_| Err("Task panicked".to_string()))
        };
        if let Some(source) = &task.source {
            state
                .queue
                .lock()
                .expect("task queue lock poisoned")
                .busy_sources
                .remove(source);
        }
        state.registry.complete(task.task_id, result);
        state.changed.notify_all();
    }
}

fn next_task(state: &HeavyQueueState) -> Option<QueuedHeavyTask> {
    let mut queue = state
        .queue
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    loop {
        if let Some(position) = queue.tasks.iter().position(|task| {
            task.source
                .as_ref()
                .is_none_or(|source| !queue.busy_sources.contains(source))
        }) {
            let task = queue
                .tasks
                .remove(position)
                .expect("queued task position is valid");
            if let Some(source) = &task.source {
                queue.busy_sources.insert(source.clone());
            }
            return Some(task);
        }
        if state.shutdown.load(Ordering::Acquire) {
            return None;
        }
        queue = state
            .changed
            .wait(queue)
            .unwrap_or_else(|error| error.into_inner());
    }
}
