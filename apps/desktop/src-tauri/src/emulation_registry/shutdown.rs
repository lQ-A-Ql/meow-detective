//! shutdown for emulation sessions.
use super::*;
impl EmulationRegistry {
    pub fn cleanup_case(&self, case_id: &str) -> Result<(), EmulationRegistryError> {
        self.cleanup_case_on_large_stack(case_id)
    }

    pub fn cleanup_source(
        &self,
        case_id: &str,
        data_source_id: &str,
    ) -> Result<(), EmulationRegistryError> {
        self.cleanup_source_on_large_stack(case_id, data_source_id)
    }

    pub(crate) fn cleanup_case_inner(&self, case_id: &str) -> Result<(), EmulationRegistryError> {
        self.cleanup_matching(|entry| entry.case_id == case_id)
    }

    pub(crate) fn cleanup_source_inner(
        &self,
        case_id: &str,
        data_source_id: &str,
    ) -> Result<(), EmulationRegistryError> {
        self.cleanup_matching(|entry| {
            entry.case_id == case_id && entry.status.data_source_id == data_source_id
        })
    }

    fn cleanup_matching(
        &self,
        predicate: impl Fn(&EmulationEntry) -> bool,
    ) -> Result<(), EmulationRegistryError> {
        let ids = self
            .entries
            .lock()
            .map_err(|_| Self::lock_error())?
            .iter()
            .filter(|(_, entry)| predicate(entry) && entry.status.state != EmulationState::Released)
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>();
        let mut failures = Vec::new();
        for id in &ids {
            if let Err(error) = self.release(id) {
                tracing::warn!(session_id = %id, error = %error, "emulation session release failed during cleanup");
                failures.push(format!("{id}: {error}"));
            }
        }
        if failures.is_empty() {
            Ok(())
        } else {
            Err(EmulationRegistryError::Backend(format!(
                "{} of {} sessions failed to release: {}",
                failures.len(),
                ids.len(),
                failures.join("; ")
            )))
        }
    }
}
impl Drop for EmulationRegistry {
    fn drop(&mut self) {
        if Arc::strong_count(&self.entries) != 1 {
            return;
        }
        let ids = self
            .entries
            .lock()
            .map(|entries| entries.keys().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        for id in ids {
            if let Err(error) = self.release(&id) {
                tracing::warn!(session_id = %id, error = %error, "emulation session release failed during registry drop");
            }
        }
    }
}
