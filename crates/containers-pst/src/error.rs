#[derive(Debug, thiserror::Error)]
pub enum PstError {
    #[error("message limit exceeded: {0}")]
    MessageLimit(&'static str),
    #[error("invalid message: {0}")]
    Message(&'static str),
    #[error("invalid PST format: {0}")]
    InvalidFormat(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("unsupported feature: {0}")]
    Unsupported(String),
    #[error("mbox parse error: {0}")]
    MboxError(String),
}
