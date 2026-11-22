//! Process identity for reclaiming abandoned persisted execution attempts.

use serde::{Deserialize, Serialize};
use std::{io, sync::OnceLock};
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProcessOwner {
    pid: u32,
    started_at: u64,
}

pub fn current_owner() -> io::Result<String> {
    static OWNER: OnceLock<String> = OnceLock::new();
    if let Some(owner) = OWNER.get() {
        return Ok(owner.clone());
    }
    let pid = std::process::id();
    let started_at = start_time(pid)?
        .ok_or_else(|| io::Error::other("current process identity is unavailable"))?;
    let encoded =
        serde_json::to_string(&ProcessOwner { pid, started_at }).map_err(io::Error::other)?;
    let _ = OWNER.set(encoded.clone());
    Ok(encoded)
}

pub fn owner_is_running(encoded: &str) -> io::Result<bool> {
    let owner: ProcessOwner = serde_json::from_str(encoded).map_err(io::Error::other)?;
    Ok(start_time(owner.pid)? == Some(owner.started_at))
}

fn start_time(pid: u32) -> io::Result<Option<u64>> {
    let mut system = System::new().map_err(io::Error::other)?;
    let pid = Pid::from_u32(pid);
    system.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[pid]),
        true,
        ProcessRefreshKind::nothing().without_tasks(),
    );
    Ok(system.process(pid).map(|process| process.start_time()))
}
