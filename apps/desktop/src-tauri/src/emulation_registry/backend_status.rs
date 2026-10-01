//! backend status for emulation sessions.
use super::*;
pub(super) fn refresh_backend(entry: &mut EmulationEntry) {
    entry.status.maintenance_media =
        entry.status.maintenance_media && entry.workspace.maintenance_iso_present();
    if matches!(
        entry.status.state,
        EmulationState::Released | EmulationState::FailedCleanupPending | EmulationState::Quiescing
    ) {
        // Quiescing legitimately has its handles checked out by `release`.
        return;
    }
    let Some(backend) = entry.backend.as_ref() else {
        entry.status.state = EmulationState::FailedCleanupPending;
        entry.status.error = Some("emulation mount backend handle is missing".to_string());
        return;
    };
    match backend.poll_exit() {
        Ok(None) => {}
        Ok(Some(error)) => {
            entry.status.state = EmulationState::FailedCleanupPending;
            entry.status.error = Some(error);
        }
        Err(error) => {
            entry.status.state = EmulationState::FailedCleanupPending;
            entry.status.error = Some(error.to_string());
        }
    }
    refresh_guest_phase(entry);
}
