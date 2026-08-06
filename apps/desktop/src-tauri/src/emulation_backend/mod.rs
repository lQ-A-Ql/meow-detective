use std::path::Path;
use std::sync::Arc;

use evidence_emulation::CowDisk;
use thiserror::Error;

#[cfg(windows)]
mod dokan;

#[derive(Debug, Error)]
pub(crate) enum EmulationBackendError {
    #[error("evidence emulation is unsupported on this platform")]
    #[cfg(not(windows))]
    UnsupportedPlatform,
    #[error("emulation mount point is invalid: {0}")]
    InvalidMountPoint(String),
    #[error("emulation mount backend failed: {0}")]
    Backend(String),
    #[error("emulation mount backend startup timed out")]
    StartupTimeout,
}

pub(crate) struct EmulationBackendHandle {
    #[cfg(windows)]
    inner: dokan::DokanExtentMount,
}

impl EmulationBackendHandle {
    #[cfg(windows)]
    pub(crate) fn poll_exit(&self) -> Result<Option<String>, EmulationBackendError> {
        self.inner.poll_exit()
    }

    #[cfg(not(windows))]
    pub(crate) fn poll_exit(&self) -> Result<Option<String>, EmulationBackendError> {
        let _ = self;
        Err(EmulationBackendError::UnsupportedPlatform)
    }

    #[cfg(windows)]
    pub(crate) fn stop(&self) -> Result<(), EmulationBackendError> {
        self.inner.stop()
    }

    #[cfg(not(windows))]
    pub(crate) fn stop(&self) -> Result<(), EmulationBackendError> {
        let _ = self;
        Err(EmulationBackendError::UnsupportedPlatform)
    }
}

#[cfg(windows)]
pub(crate) fn start(
    disk: Arc<CowDisk>,
    session_root: &Path,
    mount_point: &Path,
) -> Result<EmulationBackendHandle, EmulationBackendError> {
    dokan::start(disk, session_root, mount_point).map(|inner| EmulationBackendHandle { inner })
}

#[cfg(not(windows))]
pub(crate) fn start(
    disk: Arc<CowDisk>,
    session_root: &Path,
    mount_point: &Path,
) -> Result<EmulationBackendHandle, EmulationBackendError> {
    let _ = (disk, session_root, mount_point);
    Err(EmulationBackendError::UnsupportedPlatform)
}
