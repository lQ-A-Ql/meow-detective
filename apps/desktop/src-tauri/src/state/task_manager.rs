//! Background task lifecycle management.
//!
//! Manages background tasks (like file import) with support for:
//! - Task registration and tracking
//! - Cancellation via AtomicBool tokens
//! - Cleanup of finished tasks
//! - Graceful shutdown of all tasks

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// Result type for background tasks.
pub type TaskResult = Result<(), String>;

/// Information about a running task.
struct TaskEntry {
    handle: JoinHandle<TaskResult>,
    cancel_token: Arc<AtomicBool>,
    started_at: Instant,
}

/// Manager for background tasks with cancellation support.
pub struct TaskManager {
    tasks: Mutex<HashMap<String, TaskEntry>>,
}

impl TaskManager {
    /// Create a new TaskManager.
    pub fn new() -> Self {
        Self {
            tasks: Mutex::new(HashMap::new()),
        }
    }

    /// Register a new background task.
    ///
    /// Returns the cancel token for the task.
    pub fn register(&self, task_id: String, handle: JoinHandle<TaskResult>) -> Arc<AtomicBool> {
        let cancel_token = Arc::new(AtomicBool::new(false));
        let entry = TaskEntry {
            handle,
            cancel_token: cancel_token.clone(),
            started_at: Instant::now(),
        };

        let mut tasks = self.tasks.lock().unwrap_or_else(|e| e.into_inner());
        tasks.insert(task_id, entry);

        cancel_token
    }

    /// Register a new background task with an existing cancel token.
    ///
    /// This allows the caller to provide their own cancel token
    /// that the task can check for cancellation.
    pub fn register_with_token(
        &self,
        task_id: String,
        handle: JoinHandle<TaskResult>,
        cancel_token: Arc<AtomicBool>,
    ) {
        let entry = TaskEntry {
            handle,
            cancel_token: cancel_token.clone(),
            started_at: Instant::now(),
        };

        let mut tasks = self.tasks.lock().unwrap_or_else(|e| e.into_inner());
        tasks.insert(task_id, entry);
    }

    /// Cancel a specific task by ID.
    ///
    /// Returns true if the task was found and cancelled.
    pub fn cancel(&self, task_id: &str) -> bool {
        let tasks = self.tasks.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(entry) = tasks.get(task_id) {
            entry.cancel_token.store(true, Ordering::Relaxed);
            true
        } else {
            false
        }
    }

    /// Cancel all running tasks.
    pub fn cancel_all(&self) {
        let tasks = self.tasks.lock().unwrap_or_else(|e| e.into_inner());
        for (_, entry) in tasks.iter() {
            entry.cancel_token.store(true, Ordering::Relaxed);
        }
    }

    /// Wait for all tasks to complete.
    ///
    /// This will block until all tasks finish or the timeout is reached.
    pub fn wait_all(&self, timeout: Duration) -> Vec<(String, TaskResult)> {
        let task_ids: Vec<String> = {
            let tasks = self.tasks.lock().unwrap_or_else(|e| e.into_inner());
            tasks.keys().cloned().collect()
        };

        let start = Instant::now();
        let mut results = Vec::new();

        for task_id in task_ids {
            let remaining = timeout.saturating_sub(start.elapsed());
            if remaining.is_zero() {
                break;
            }

            if let Some(result) = self.wait_task(&task_id, remaining) {
                results.push((task_id, result));
            }
        }

        results
    }

    /// Wait for a specific task to complete.
    pub fn wait_task(&self, task_id: &str, timeout: Duration) -> Option<TaskResult> {
        let entry = {
            let mut tasks = self.tasks.lock().unwrap_or_else(|e| e.into_inner());
            tasks.remove(task_id)?
        };

        // Wait with timeout
        let start = Instant::now();
        loop {
            if entry.handle.is_finished() {
                break;
            }
            if start.elapsed() >= timeout {
                // Re-insert the task if it didn't finish
                let mut tasks = self.tasks.lock().unwrap_or_else(|e| e.into_inner());
                tasks.insert(task_id.to_string(), entry);
                return None;
            }
            std::thread::sleep(Duration::from_millis(10));
        }

        match entry.handle.join() {
            Ok(result) => Some(result),
            Err(_) => Some(Err("Task panicked".to_string())),
        }
    }

    /// Remove finished tasks from the manager.
    ///
    /// Returns the number of tasks removed.
    pub fn cleanup_finished(&self) -> usize {
        let mut tasks = self.tasks.lock().unwrap_or_else(|e| e.into_inner());
        let initial_count = tasks.len();
        tasks.retain(|_, entry| !entry.handle.is_finished());
        initial_count - tasks.len()
    }

    /// Get a list of running task IDs.
    pub fn running_tasks(&self) -> Vec<String> {
        let tasks = self.tasks.lock().unwrap_or_else(|e| e.into_inner());
        tasks.keys().cloned().collect()
    }

    /// Get the number of running tasks.
    pub fn task_count(&self) -> usize {
        let tasks = self.tasks.lock().unwrap_or_else(|e| e.into_inner());
        tasks.len()
    }

    /// Check if a specific task is running.
    pub fn is_running(&self, task_id: &str) -> bool {
        let tasks = self.tasks.lock().unwrap_or_else(|e| e.into_inner());
        tasks.contains_key(task_id)
    }

    /// Get the elapsed time for a task.
    pub fn task_elapsed(&self, task_id: &str) -> Option<Duration> {
        let tasks = self.tasks.lock().unwrap_or_else(|e| e.into_inner());
        tasks.get(task_id).map(|entry| entry.started_at.elapsed())
    }

    /// Check if a task has been cancelled.
    pub fn is_cancelled(&self, task_id: &str) -> bool {
        let tasks = self.tasks.lock().unwrap_or_else(|e| e.into_inner());
        tasks
            .get(task_id)
            .map(|entry| entry.cancel_token.load(Ordering::Relaxed))
            .unwrap_or(false)
    }

    /// Get the cancel token for a task.
    ///
    /// Returns None if the task doesn't exist.
    pub fn get_cancel_token(&self, task_id: &str) -> Option<Arc<AtomicBool>> {
        let tasks = self.tasks.lock().unwrap_or_else(|e| e.into_inner());
        tasks.get(task_id).map(|entry| entry.cancel_token.clone())
    }
}

impl Default for TaskManager {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for TaskManager {
    fn drop(&mut self) {
        // Cancel all tasks on drop
        self.cancel_all();
    }
}

#[cfg(test)]
#[path = "../../tests/unit/state/task_manager.rs"]
mod tests;
