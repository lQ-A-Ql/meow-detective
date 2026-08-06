//! Utilman logon bypass: swaps `Windows\System32\utilman.exe` for `cmd.exe`
//! so the lock-screen "ease of access" button yields a SYSTEM shell. All
//! writes happen on the emulation overlay; the evidence image is untouched.

use std::fs;
use std::io;
use std::path::Path;

use crate::sys::{clear_readonly, is_reparse_point};
use crate::MaintenanceError;

const UTILMAN: &str = "Windows/System32/utilman.exe";
const UTILMAN_BACKUP: &str = "Windows/System32/utilman.exe.meowbak";
const COMMAND_SHELL: &str = "Windows/System32/cmd.exe";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BypassState {
    NotApplied,
    Applied,
}

pub fn inspect_bypass(windows_root: &Path) -> Result<BypassState, MaintenanceError> {
    if windows_root.join(UTILMAN_BACKUP).is_file() {
        return Ok(BypassState::Applied);
    }
    Ok(BypassState::NotApplied)
}

pub fn apply_bypass(windows_root: &Path) -> Result<BypassState, MaintenanceError> {
    let utilman = windows_root.join(UTILMAN);
    let backup = windows_root.join(UTILMAN_BACKUP);
    let shell = windows_root.join(COMMAND_SHELL);
    if backup.exists() {
        return Err(MaintenanceError::BypassBackupExists);
    }
    for required in [&utilman, &shell] {
        let metadata = fs::symlink_metadata(required).map_err(|error| {
            if error.kind() == io::ErrorKind::NotFound {
                MaintenanceError::BypassTargetMissing
            } else {
                MaintenanceError::Io(error)
            }
        })?;
        if !metadata.is_file() || is_reparse_point(&metadata) {
            return Err(MaintenanceError::BypassTargetMissing);
        }
    }
    clear_readonly(&utilman)?;
    fs::rename(&utilman, &backup)?;
    if let Err(error) = fs::copy(&shell, &utilman) {
        let _ = fs::rename(&backup, &utilman);
        return Err(error.into());
    }
    Ok(BypassState::Applied)
}

pub fn restore_bypass(windows_root: &Path) -> Result<BypassState, MaintenanceError> {
    let utilman = windows_root.join(UTILMAN);
    let backup = windows_root.join(UTILMAN_BACKUP);
    if !backup.is_file() {
        return Err(MaintenanceError::BypassBackupMissing);
    }
    if utilman.exists() {
        clear_readonly(&utilman)?;
        fs::remove_file(&utilman)?;
    }
    fs::rename(&backup, &utilman)?;
    Ok(BypassState::NotApplied)
}
