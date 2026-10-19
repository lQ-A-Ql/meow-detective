#[derive(Debug, thiserror::Error)]
pub enum UpdateError {
    #[error("update I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid update JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("update signature verification failed")]
    Signature,
    #[error("invalid portable package: {0}")]
    Invalid(&'static str),
    #[error("another update is pending recovery")]
    PendingRecovery,
}
