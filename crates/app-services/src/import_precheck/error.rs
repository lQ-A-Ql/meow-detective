use crate::datasource_service::DataSourceError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ImportSourceConfigError {
    #[error("data source platform must be windows, linux, or android")]
    UnsupportedPlatform,
    #[error("Android sparse images must use platform android")]
    AndroidSparsePlatformMismatch,
    #[error("sourcePath must exist and be accessible before import")]
    MissingOrInaccessibleSource,
    #[error("sourcePath must point to a directory or regular image file")]
    UnsupportedSourceType,
    #[error(transparent)]
    Classification(#[from] DataSourceError),
}

impl transport::ServiceErrorCategory for ImportSourceConfigError {
    fn category(&self) -> transport::ErrorCategory {
        match self {
            Self::MissingOrInaccessibleSource
            | Self::UnsupportedSourceType
            | Self::AndroidSparsePlatformMismatch => transport::ErrorCategory::Validation,
            Self::UnsupportedPlatform => transport::ErrorCategory::Unsupported,
            Self::Classification(error) => error.category(),
        }
    }
}

impl ImportSourceConfigError {
    pub fn is_invalid_input(&self) -> bool {
        matches!(
            self,
            Self::MissingOrInaccessibleSource
                | Self::UnsupportedSourceType
                | Self::AndroidSparsePlatformMismatch
        )
    }
}
