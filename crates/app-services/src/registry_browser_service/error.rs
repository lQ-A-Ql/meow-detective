use artifacts_windows::registry::browser::BrowserError;
use thiserror::Error;
use transport::{ErrorCategory, ServiceErrorCategory};

#[derive(Debug, Error)]
pub enum RegistryBrowserError {
    #[error("registry request is invalid")]
    InvalidRequest,
    #[error("registry session has expired")]
    ExpiredSession,
    #[error("registry source is unavailable")]
    SourceUnavailable,
    #[error("registry hive is locked")]
    LockedHive,
    #[error("registry browser memory budget is exhausted")]
    Busy,
    #[error("registry runtime is unavailable")]
    Internal,
    #[error(transparent)]
    Parser(#[from] BrowserError),
}
impl ServiceErrorCategory for RegistryBrowserError {
    fn category(&self) -> ErrorCategory {
        match self {
            Self::InvalidRequest | Self::ExpiredSession => ErrorCategory::Validation,
            Self::SourceUnavailable => ErrorCategory::Io,
            Self::LockedHive => ErrorCategory::Security,
            Self::Parser(BrowserError::Unsupported | BrowserError::Limit) => {
                ErrorCategory::Unsupported
            }
            Self::Parser(_) => ErrorCategory::Parser,
            Self::Busy => ErrorCategory::Validation,
            Self::Internal => ErrorCategory::Internal,
        }
    }
    fn code(&self) -> Option<&'static str> {
        Some(match self {
            Self::InvalidRequest
            | Self::Parser(BrowserError::InvalidPath | BrowserError::Limit) => "invalidRequest",
            Self::ExpiredSession => "expiredSession",
            Self::SourceUnavailable => "sourceUnavailable",
            Self::LockedHive => "lockedHive",
            Self::Parser(BrowserError::NotFound) => "keyNotFound",
            Self::Parser(BrowserError::Corrupt) => "corruptKey",
            Self::Parser(BrowserError::Unsupported) => "unsupportedHive",
            Self::Busy => "browserBusy",
            Self::Internal => "failed",
        })
    }
    fn user_message(&self) -> Option<&'static str> {
        Some("Registry browser request could not be completed.")
    }
    fn recoverable(&self) -> Option<bool> {
        Some(true)
    }
}
