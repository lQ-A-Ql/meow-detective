#[derive(Debug, thiserror::Error)]
pub enum BoltReadError {
    #[error("Bolt input exceeds the supported byte limit")]
    InputLimit,
    #[error("Bolt input path is invalid")]
    InvalidInput,
    #[error("Bolt reader runtime is unavailable")]
    MissingWorker,
    #[error("Bolt reading was cancelled")]
    Cancelled,
    #[error("Bolt reader exceeded its time limit")]
    Timeout,
    #[error("Bolt reader exceeded its output limit")]
    OutputLimit,
    #[error("Bolt reader process failed")]
    WorkerFailed,
    #[error("Bolt reader response is invalid")]
    InvalidResponse,
    #[error("Bolt input could not be decoded")]
    Decode,
    #[error("Bolt worker IO failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("Bolt worker protocol failed")]
    Protocol(#[from] serde_json::Error),
    #[error("{0}")]
    Budget(#[from] crate::resource_budget::ResourceError),
}

impl transport::ServiceErrorCategory for BoltReadError {
    fn recoverable(&self) -> Option<bool> {
        matches!(
            self,
            Self::Cancelled
                | Self::Timeout
                | Self::Budget(crate::resource_budget::ResourceError::Busy)
        )
        .then_some(true)
    }
    fn category(&self) -> transport::ErrorCategory {
        use transport::ErrorCategory;
        match self {
            Self::Cancelled => ErrorCategory::Cancelled,
            Self::Timeout => ErrorCategory::Timeout,
            Self::InputLimit | Self::OutputLimit => ErrorCategory::Unsupported,
            Self::InvalidInput => ErrorCategory::Validation,
            Self::MissingWorker | Self::WorkerFailed => ErrorCategory::External,
            Self::Decode => ErrorCategory::Parser,
            Self::InvalidResponse | Self::Protocol(_) => ErrorCategory::Internal,
            Self::Io(_) | Self::Budget(_) => ErrorCategory::Io,
        }
    }
}
