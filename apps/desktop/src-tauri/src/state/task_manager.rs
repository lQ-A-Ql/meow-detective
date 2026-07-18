//! Background task lifecycle management.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

mod heavy_queue;
mod registry;

use heavy_queue::HeavyTaskQueue;
use registry::{cancel_and_collect, thread_name, validate_registration, TaskEntry, TaskRegistry};

pub use registry::{TaskRegistrationError, TaskResult, TaskScope};

pub struct TaskManager {
    registry: Arc<TaskRegistry>,
    heavy_queue: HeavyTaskQueue,
}

impl TaskManager {
    pub fn new() -> Self {
        let registry = Arc::new(TaskRegistry::default());
        let heavy_queue = HeavyTaskQueue::new(Arc::clone(&registry));
        Self {
            registry,
            heavy_queue,
        }
    }

    pub fn spawn<F>(
        &self,
        task_id: String,
        task: F,
    ) -> Result<Arc<AtomicBool>, TaskRegistrationError>
    where
        F: FnOnce(Arc<AtomicBool>) -> TaskResult + Send + 'static,
    {
        let cancel_token = Arc::new(AtomicBool::new(false));
        let worker_token = Arc::clone(&cancel_token);
        self.spawn_internal(task_id, None, Arc::clone(&cancel_token), false, move || {
            task(worker_token)
        })?;
        Ok(cancel_token)
    }

    pub fn spawn_scoped<F>(
        &self,
        task_id: String,
        scope: TaskScope,
        cancel_token: Arc<AtomicBool>,
        task: F,
    ) -> Result<(), TaskRegistrationError>
    where
        F: FnOnce() -> TaskResult + Send + 'static,
    {
        self.spawn_internal(task_id, Some(scope), cancel_token, false, task)
    }

    pub fn spawn_scoped_heavy<F>(
        &self,
        task_id: String,
        scope: TaskScope,
        cancel_token: Arc<AtomicBool>,
        task: F,
    ) -> Result<(), TaskRegistrationError>
    where
        F: FnOnce() -> TaskResult + Send + 'static,
    {
        self.spawn_internal(task_id, Some(scope), cancel_token, true, task)
    }

    fn spawn_internal<F>(
        &self,
        task_id: String,
        scope: Option<TaskScope>,
        cancel_token: Arc<AtomicBool>,
        heavy: bool,
        task: F,
    ) -> Result<(), TaskRegistrationError>
    where
        F: FnOnce() -> TaskResult + Send + 'static,
    {
        {
            let mut state = self
                .registry
                .state
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            validate_registration(&state, &task_id, scope.as_ref())?;
            state.completed.remove(&task_id);
            state
                .completed_order
                .retain(|candidate| candidate != &task_id);
            state.tasks.insert(
                task_id.clone(),
                TaskEntry {
                    cancel_token: Arc::clone(&cancel_token),
                    started_at: Instant::now(),
                    scope,
                },
            );
        }

        if heavy {
            if self
                .heavy_queue
                .enqueue(task_id.clone(), cancel_token, task)
                .is_ok()
            {
                return Ok(());
            }
            let mut state = self
                .registry
                .state
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            state.tasks.remove(&task_id);
            return Err(TaskRegistrationError::HeavyQueueClosed(task_id));
        }

        let registry = Arc::clone(&self.registry);
        let worker_task_id = task_id.clone();
        if let Err(error) = std::thread::Builder::new()
            .name(thread_name(&task_id))
            .spawn(move || {
                let result = catch_unwind(AssertUnwindSafe(task))
                    .unwrap_or_else(|_| Err("Task panicked".to_string()));
                registry.complete(worker_task_id, result);
            })
        {
            let mut state = self
                .registry
                .state
                .lock()
                .unwrap_or_else(|lock_error| lock_error.into_inner());
            state.tasks.remove(&task_id);
            return Err(TaskRegistrationError::Spawn(error));
        }
        Ok(())
    }

    pub fn cancel(&self, task_id: &str) -> bool {
        let task_ids = {
            let state = self
                .registry
                .state
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            state
                .tasks
                .iter()
                .filter_map(|(candidate_id, entry)| {
                    let matches_group = entry
                        .scope
                        .as_ref()
                        .is_some_and(|scope| scope.group_id == task_id);
                    if candidate_id == task_id || matches_group {
                        entry.cancel_token.store(true, Ordering::Release);
                        Some(candidate_id.clone())
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
        };
        self.heavy_queue.cancel_queued(&task_ids);
        !task_ids.is_empty()
    }

    pub fn retire_case_and_drain(
        &self,
        case_id: &str,
        timeout: Duration,
    ) -> Vec<(String, TaskResult)> {
        let task_ids = {
            let mut state = self
                .registry
                .state
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            state.retired_cases.insert(case_id.to_string());
            cancel_and_collect(&state.tasks, |scope| scope.case_id == case_id)
        };
        self.heavy_queue.cancel_queued(&task_ids);
        self.wait_task_ids(task_ids, timeout)
    }

    pub fn retire_source_and_drain(
        &self,
        case_id: &str,
        data_source_id: &str,
        timeout: Duration,
    ) -> Vec<(String, TaskResult)> {
        let task_ids = {
            let mut state = self
                .registry
                .state
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            state
                .retired_sources
                .insert((case_id.to_string(), data_source_id.to_string()));
            cancel_and_collect(&state.tasks, |scope| {
                scope.case_id == case_id && scope.data_source_id.as_deref() == Some(data_source_id)
            })
        };
        self.heavy_queue.cancel_queued(&task_ids);
        self.wait_task_ids(task_ids, timeout)
    }

    pub fn reactivate_case(&self, case_id: &str) {
        let mut state = self
            .registry
            .state
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        state.retired_cases.remove(case_id);
    }

    pub fn reactivate_source(&self, case_id: &str, data_source_id: &str) {
        let mut state = self
            .registry
            .state
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        state
            .retired_sources
            .remove(&(case_id.to_string(), data_source_id.to_string()));
    }

    pub fn cancel_all(&self) {
        let state = self
            .registry
            .state
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        for entry in state.tasks.values() {
            entry.cancel_token.store(true, Ordering::Release);
        }
    }

    pub fn wait_all(&self, timeout: Duration) -> Vec<(String, TaskResult)> {
        let task_ids = self.task_ids_matching(|_| true);
        self.wait_task_ids(task_ids, timeout)
    }

    fn wait_task_ids(&self, task_ids: Vec<String>, timeout: Duration) -> Vec<(String, TaskResult)> {
        let started = Instant::now();
        let mut results = Vec::new();
        for task_id in task_ids {
            let remaining = timeout.saturating_sub(started.elapsed());
            if remaining.is_zero() {
                break;
            }
            if let Some(result) = self.wait_task(&task_id, remaining) {
                results.push((task_id, result));
            }
        }
        results
    }

    pub fn wait_task(&self, task_id: &str, timeout: Duration) -> Option<TaskResult> {
        let started = Instant::now();
        let mut state = self
            .registry
            .state
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        loop {
            if let Some(result) = state.completed.remove(task_id) {
                state
                    .completed_order
                    .retain(|candidate| candidate != task_id);
                return Some(result);
            }
            if !state.tasks.contains_key(task_id) {
                return None;
            }
            let remaining = timeout.saturating_sub(started.elapsed());
            if remaining.is_zero() {
                return None;
            }
            let (next, wait) = self
                .registry
                .changed
                .wait_timeout(state, remaining)
                .unwrap_or_else(|error| error.into_inner());
            state = next;
            if wait.timed_out() && state.tasks.contains_key(task_id) {
                return None;
            }
        }
    }

    pub fn running_tasks(&self) -> Vec<String> {
        self.task_ids_matching(|_| true)
    }

    pub fn task_count(&self) -> usize {
        let state = self
            .registry
            .state
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        state.tasks.len()
    }

    pub fn task_count_for_case(&self, case_id: &str) -> usize {
        self.count_matching(|scope| scope.case_id == case_id)
    }

    pub fn task_count_for_case_wide(&self, case_id: &str) -> usize {
        self.count_matching(|scope| scope.case_id == case_id && scope.data_source_id.is_none())
    }

    pub fn task_count_for_data_source(&self, case_id: &str, data_source_id: &str) -> usize {
        self.count_matching(|scope| {
            scope.case_id == case_id && scope.data_source_id.as_deref() == Some(data_source_id)
        })
    }

    pub fn is_running(&self, task_id: &str) -> bool {
        let state = self
            .registry
            .state
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        state.tasks.contains_key(task_id)
    }

    pub fn task_elapsed(&self, task_id: &str) -> Option<Duration> {
        let state = self
            .registry
            .state
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        state
            .tasks
            .get(task_id)
            .map(|entry| entry.started_at.elapsed())
    }

    pub fn is_cancelled(&self, task_id: &str) -> bool {
        let state = self
            .registry
            .state
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        state
            .tasks
            .get(task_id)
            .is_some_and(|entry| entry.cancel_token.load(Ordering::Acquire))
    }

    pub fn get_cancel_token(&self, task_id: &str) -> Option<Arc<AtomicBool>> {
        let state = self
            .registry
            .state
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        state
            .tasks
            .get(task_id)
            .map(|entry| Arc::clone(&entry.cancel_token))
    }

    fn count_matching(&self, predicate: impl Fn(&TaskScope) -> bool) -> usize {
        let state = self
            .registry
            .state
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        state
            .tasks
            .values()
            .filter(|entry| entry.scope.as_ref().is_some_and(&predicate))
            .count()
    }

    fn task_ids_matching(&self, predicate: impl Fn(&TaskEntry) -> bool) -> Vec<String> {
        let state = self
            .registry
            .state
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        state
            .tasks
            .iter()
            .filter_map(|(task_id, entry)| predicate(entry).then_some(task_id.clone()))
            .collect()
    }
}

impl Default for TaskManager {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for TaskManager {
    fn drop(&mut self) {
        self.cancel_all();
        self.heavy_queue.shutdown();
    }
}

#[cfg(test)]
#[path = "../../tests/unit/state/task_manager.rs"]
mod tests;
