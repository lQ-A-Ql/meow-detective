use app_services::active_case::ActiveCase;
use std::sync::{Arc, Mutex};

use super::task_manager::TaskManager;

/// Application state shared across Tauri commands.
#[derive(Clone)]
pub struct AppState {
    /// Currently active case (if any).
    pub active_case: Arc<Mutex<Option<ActiveCase>>>,
    /// Manager for background tasks.
    pub task_manager: Arc<TaskManager>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            active_case: Arc::new(Mutex::new(None)),
            task_manager: Arc::new(TaskManager::new()),
        }
    }
}
