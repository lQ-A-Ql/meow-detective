use super::{registry::TaskRegistry, TaskManager};
use std::sync::Arc;

pub struct UpdateTaskGuard(Arc<TaskRegistry>);
impl TaskManager {
    /// Checking for idle and closing admission happen under the same registry lock.
    pub fn begin_update(&self) -> Option<UpdateTaskGuard> {
        let mut state = self
            .registry
            .state
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if state.updating || !state.tasks.is_empty() {
            return None;
        }
        state.updating = true;
        Some(UpdateTaskGuard(Arc::clone(&self.registry)))
    }
}
impl Drop for UpdateTaskGuard {
    fn drop(&mut self) {
        self.0
            .state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .updating = false;
    }
}
