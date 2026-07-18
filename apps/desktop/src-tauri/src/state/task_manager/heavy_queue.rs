use std::collections::{HashSet, VecDeque};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};

use super::registry::{TaskRegistry, TaskResult};

type HeavyTaskBody = Box<dyn FnOnce() -> TaskResult + Send + 'static>;

struct QueuedHeavyTask {
    task_id: String,
    cancel_token: Arc<AtomicBool>,
    body: HeavyTaskBody,
}

struct HeavyQueueState {
    queue: Mutex<VecDeque<QueuedHeavyTask>>,
    changed: Condvar,
    shutdown: AtomicBool,
    registry: Arc<TaskRegistry>,
}

pub(super) struct HeavyTaskQueue {
    state: Arc<HeavyQueueState>,
}

impl HeavyTaskQueue {
    pub(super) fn new(registry: Arc<TaskRegistry>) -> Self {
        let state = Arc::new(HeavyQueueState {
            queue: Mutex::new(VecDeque::new()),
            changed: Condvar::new(),
            shutdown: AtomicBool::new(false),
            registry,
        });
        let worker_state = Arc::clone(&state);
        std::thread::Builder::new()
            .name("meow-heavy-worker".to_string())
            .spawn(move || run_worker(worker_state))
            .expect("heavy background worker must start");
        Self { state }
    }

    pub(super) fn enqueue<F>(
        &self,
        task_id: String,
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
        queue.push_back(QueuedHeavyTask {
            task_id,
            cancel_token,
            body: Box::new(body),
        });
        self.state.changed.notify_one();
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
            queue.retain(|task| {
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
        state.registry.complete(task.task_id, result);
    }
}

fn next_task(state: &HeavyQueueState) -> Option<QueuedHeavyTask> {
    let mut queue = state
        .queue
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    loop {
        if let Some(task) = queue.pop_front() {
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
