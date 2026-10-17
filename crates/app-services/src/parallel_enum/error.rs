use thiserror::Error;

/// Typed error for parallel filesystem enumeration.
#[derive(Debug, Error)]
pub enum ParallelEnumError {
    #[error("Cancelled")]
    Cancelled,
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("MFT parameter error: {0}")]
    MftParams(String),
    #[error("NTFS materialization resource limit: {0}")]
    MftResourceLimit(&'static str),
    #[error("Database error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("Staging persistence error: {0}")]
    Staging(#[from] persistence_sqlite::DbError),
    #[error("Invalid deferred NTFS partition plan")]
    InvalidPlan,
    #[error("Filesystem enumeration error: {0}")]
    Filesystem(String),
}

impl From<String> for ParallelEnumError {
    fn from(msg: String) -> Self {
        Self::MftParams(msg)
    }
}

impl From<&str> for ParallelEnumError {
    fn from(msg: &str) -> Self {
        Self::MftParams(msg.to_owned())
    }
}

impl ParallelEnumError {
    pub(super) fn allows_fallback(&self) -> bool {
        match self {
            Self::MftParams(_) => true,
            Self::Io(error) => !error.get_ref().is_some_and(|cause| {
                cause.is::<crate::source_runtime::SourceInvalidated>()
                    || cause.is::<crate::resource_budget::ResourceError>()
            }),
            _ => false,
        }
    }

    pub(super) fn recursive(message: String, cancelled: bool) -> Self {
        if cancelled && message == "Cancelled" {
            Self::Cancelled
        } else {
            Self::Filesystem(message)
        }
    }
}

impl transport::ServiceErrorCategory for ParallelEnumError {
    fn category(&self) -> transport::ErrorCategory {
        match self {
            Self::Cancelled => transport::ErrorCategory::Cancelled,
            Self::Io(_) | Self::Db(_) => transport::ErrorCategory::Io,
            Self::MftParams(_) | Self::MftResourceLimit(_) | Self::InvalidPlan => {
                transport::ErrorCategory::Validation
            }
            Self::Staging(error) => transport::ServiceErrorCategory::category(error),
            Self::Filesystem(_) => transport::ErrorCategory::Parser,
        }
    }
}
