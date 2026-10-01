//! guest phase for emulation sessions.
use super::*;
const GUEST_SIGNAL_WINDOW: Duration = Duration::from_secs(90);

pub(super) fn refresh_guest_phase(entry: &mut EmulationEntry) {
    if entry.status.state != EmulationState::Running
        || entry.status.guest_phase != EmulationGuestPhase::Booting
    {
        return;
    }
    if entry
        .vmware
        .as_ref()
        .is_some_and(VmwareControl::guest_userspace_started)
    {
        entry.status.guest_phase = EmulationGuestPhase::FilesystemMounted;
        entry.boot_started_at = None;
        return;
    }
    if entry
        .boot_started_at
        .is_some_and(|started| started.elapsed() >= GUEST_SIGNAL_WINDOW)
    {
        // VMware Tools is optional and its heartbeat is not a login-ready
        // oracle. Do not leave an unobservable guest labelled "booting"
        // forever once the bounded observation window closes.
        entry.status.guest_phase = EmulationGuestPhase::Unknown;
        entry.boot_started_at = None;
    }
}
