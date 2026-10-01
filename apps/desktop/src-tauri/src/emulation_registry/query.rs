//! query for emulation sessions.
use super::*;
impl EmulationRegistry {
    pub fn status(
        &self,
        session_id: &str,
    ) -> Result<EmulationSessionStatus, EmulationRegistryError> {
        let mut entries = self.entries.lock().map_err(|_| Self::lock_error())?;
        let entry = entries
            .get_mut(session_id)
            .ok_or_else(|| EmulationRegistryError::NotFound(session_id.to_string()))?;
        refresh_backend(entry);
        Ok(entry.status.clone())
    }

    pub fn list(&self) -> Result<Vec<EmulationSessionStatus>, EmulationRegistryError> {
        let mut entries = self.entries.lock().map_err(|_| Self::lock_error())?;
        for entry in entries.values_mut() {
            refresh_backend(entry);
        }
        let mut statuses = entries
            .values()
            .map(|entry| entry.status.clone())
            .collect::<Vec<_>>();
        statuses.sort_by(|left, right| left.session_id.cmp(&right.session_id));
        Ok(statuses)
    }
}
