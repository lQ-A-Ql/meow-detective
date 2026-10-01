//! admission for emulation sessions.
use super::*;
impl EmulationRegistry {
    pub(super) fn reject_duplicate(
        &self,
        case_root: &Path,
        data_source_id: &DataSourceId,
    ) -> Result<(), EmulationRegistryError> {
        let active_session = self
            .entries
            .lock()
            .map_err(|_| Self::lock_error())?
            .values()
            .find(|entry| {
                entry.status.data_source_id == data_source_id.0
                    && entry.status.state != EmulationState::Released
            })
            .map(|entry| entry.status.session_id.clone());
        if let Some(session_id) = active_session {
            tracing::warn!(
                data_source_id = %data_source_id.0,
                session_id = %session_id,
                "emulation prepare rejected because this process already owns an active session"
            );
            return Err(EmulationRegistryError::AlreadyActive { session_id });
        }

        // The registry is process-local. A previous application instance may
        // still own a running VM, so consult the durable provenance records
        // before allocating another COW workspace. We never stop a VM here.
        if let Some(session_id) =
            session_discovery::find_active_session(case_root, &data_source_id.0)
                .map_err(|error| EmulationRegistryError::Vmware(error.to_string()))?
        {
            tracing::warn!(
                data_source_id = %data_source_id.0,
                session_id = %session_id,
                "emulation prepare rejected because another process owns an active session"
            );
            return Err(EmulationRegistryError::AlreadyActive { session_id });
        }
        Ok(())
    }

    pub(super) fn insert_entry(
        &self,
        session_id: String,
        entry: EmulationEntry,
    ) -> Result<(), EmulationRegistryError> {
        let mut entries = self.entries.lock().map_err(|_| Self::lock_error())?;
        if entries.values().any(|current| {
            current.status.data_source_id == entry.status.data_source_id
                && current.status.state != EmulationState::Released
        }) {
            drop(entries);
            if let Some(backend) = entry.backend.as_ref() {
                let _ = backend.stop();
            }
            entry.workspace.remove_best_effort();
            return Err(EmulationRegistryError::AlreadyActive {
                session_id: entry.status.session_id.clone(),
            });
        }
        entries.insert(session_id, entry);
        Ok(())
    }
}
