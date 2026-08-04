use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum MountError {
    #[error("mount plan is invalid: {0}")]
    InvalidPlan(&'static str),
    #[error("virtual path is invalid: {0}")]
    InvalidPath(String),
    #[error("virtual path traversal is not allowed")]
    PathTraversal,
    #[error("filesystem operation failed: {0}")]
    Filesystem(String),
    #[error("path does not exist: {0}")]
    NotFound(String),
    #[error("path is a directory: {0}")]
    IsDirectory(String),
    #[error("path is not a directory: {0}")]
    NotDirectory(String),
    #[error("write access is not allowed on a forensic mount")]
    WriteDenied,
    #[error("read length {requested} exceeds the mount limit {maximum}")]
    ReadLimit { requested: usize, maximum: usize },
    #[error("read offset {offset} is beyond file size {size}")]
    OffsetOutOfBounds { offset: u64, size: u64 },
    #[error("too many open mount handles")]
    HandleLimit,
    #[error("mount handle {0} does not exist")]
    HandleNotFound(u64),
    #[error("directory page limit {requested} exceeds the mount limit {maximum}")]
    DirectoryLimit { requested: u32, maximum: u32 },
    #[error("directory page limit must be greater than zero")]
    InvalidDirectoryLimit,
    #[error("directory cursor is invalid")]
    InvalidCursor,
}
