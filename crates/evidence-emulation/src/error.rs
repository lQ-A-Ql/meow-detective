use std::path::PathBuf;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum EmulationError {
    #[error("emulation I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("parent image read failed: {0}")]
    ParentRead(#[from] evidence_block::BlockDeviceError),
    #[error("virtual disk length {0} must be non-zero and 512-byte aligned")]
    InvalidLogicalLength(u64),
    #[error("COW cluster size {0} must be a power of two between 4096 and 1048576 bytes")]
    InvalidClusterSize(u32),
    #[error("write request of {requested} bytes exceeds the {maximum}-byte limit")]
    WriteTooLarge { requested: usize, maximum: usize },
    #[error("virtual disk request [{offset}, {end}) exceeds length {length}")]
    OutOfBounds { offset: u64, end: u64, length: u64 },
    #[error("virtual disk request arithmetic overflowed")]
    ArithmeticOverflow,
    #[error("COW overlay is corrupt: {0}")]
    CorruptOverlay(String),
    #[error("COW overlay parent identity does not match the selected evidence")]
    ParentMismatch,
    #[error("COW overlay lock is poisoned")]
    LockPoisoned,
    #[error("overlay path already exists: {0}")]
    OverlayExists(PathBuf),
    #[error("invalid VMDK extent path: {0}")]
    InvalidExtentPath(String),
    #[error("invalid VMDK descriptor: {0}")]
    InvalidVmdkDescriptor(String),
    #[error("invalid VMware machine configuration: {0}")]
    InvalidVmx(String),
}
