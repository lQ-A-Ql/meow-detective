use thiserror::Error;

#[derive(Debug, Error)]
pub enum PhysicalMountError {
    #[error("physical-disk mounting is unsupported on this platform")]
    UnsupportedPlatform,
    #[error("evidence block device could not be opened: {0}")]
    BlockDevice(#[from] evidence_block::BlockDeviceError),
    #[error("local iSCSI target failed: {0}")]
    Target(#[from] iscsi_target::IscsiError),
    #[error("local iSCSI target did not start before the timeout")]
    TargetStartupTimeout,
    #[error("local iSCSI target thread panicked")]
    TargetThreadPanicked,
    #[error("starting the Microsoft iSCSI Initiator service requires an elevated application")]
    IscsiServiceRequiresElevation,
    #[error("creating a Microsoft iSCSI physical-disk session requires an elevated application")]
    IscsiLoginRequiresElevation,
    #[error("Microsoft iSCSI Initiator service did not start before the timeout")]
    IscsiServiceStartupTimeout,
    #[error("Windows iSCSI operation '{operation}' failed with code {code}")]
    WindowsApi { operation: &'static str, code: u32 },
    #[error("Windows connected the iSCSI session but did not expose a physical disk")]
    PhysicalDiskNotFound,
    #[error("iSCSI portal value is too long for Windows: {0}")]
    PortalValueTooLong(&'static str),
}
