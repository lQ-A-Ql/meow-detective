use crate::datasource_service::DataSourceError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ImportSourceConfigError {
    #[error("{0}")]
    InvalidRequest(String),
    #[error("unsupported data source platform; only Windows and Linux are supported")]
    UnsupportedPlatform,
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
            Self::InvalidRequest(_)
            | Self::MissingOrInaccessibleSource
            | Self::UnsupportedSourceType => transport::ErrorCategory::Validation,
            Self::UnsupportedPlatform => transport::ErrorCategory::Unsupported,
            Self::Classification(error) => error.category(),
        }
    }
}

impl ImportSourceConfigError {
    pub fn is_invalid_input(&self) -> bool {
        matches!(
            self,
            Self::InvalidRequest(_)
                | Self::MissingOrInaccessibleSource
                | Self::UnsupportedSourceType
        )
    }
}
