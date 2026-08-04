use evidence_mount::MountSession;
use thiserror::Error;

#[cfg(windows)]
mod dokan;

#[derive(Debug, Error)]
pub(crate) enum MountBackendError {
    #[error("read-only logical mounts are unsupported on this platform")]
    #[cfg(not(windows))]
    UnsupportedPlatform,
    #[error("mount point is invalid: {0}")]
    InvalidMountPoint(String),
    #[error("mount backend failed: {0}")]
    Backend(String),
    #[error("mount backend startup timed out")]
    StartupTimeout,
}

pub(crate) struct BackendHandle {
    #[cfg(windows)]
    inner: dokan::DokanMount,
}

impl BackendHandle {
    #[cfg(windows)]
    pub(crate) fn stop(&self) -> Result<(), MountBackendError> {
        self.inner.stop()
    }

    #[cfg(not(windows))]
    pub(crate) fn stop(&self) -> Result<(), MountBackendError> {
        let _ = self;
        Err(MountBackendError::UnsupportedPlatform)
    }

    #[cfg(windows)]
    pub(crate) fn mount_point(&self) -> String {
        self.inner.mount_point()
    }
}

#[cfg(windows)]
pub(crate) fn start(
    session: MountSession,
    requested_mount_point: Option<&str>,
) -> Result<BackendHandle, MountBackendError> {
    dokan::start(session, requested_mount_point).map(|inner| BackendHandle { inner })
}

#[cfg(not(windows))]
pub(crate) fn start(
    session: MountSession,
    requested_mount_point: Option<&str>,
) -> Result<BackendHandle, MountBackendError> {
    let _ = (session, requested_mount_point);
    Err(MountBackendError::UnsupportedPlatform)
}
