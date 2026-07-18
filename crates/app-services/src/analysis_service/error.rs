use thiserror::Error;

#[derive(Debug, Error)]
pub enum AnalysisServiceError {
    #[error("database error: {0}")]
    Db(#[from] persistence_sqlite::DbError),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("read error: {0}")]
    Read(String),
    #[error("artifact extraction error: {0}")]
    Extraction(String),
    #[error("{0} not found: {1}")]
    NotFound(&'static str, String),
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("unsupported analysis capability: {0}")]
    Unsupported(String),
    #[error("analysis extraction cancelled")]
    Cancelled,
    #[error("{0}")]
    Other(String),
}

impl From<rusqlite::Error> for AnalysisServiceError {
    fn from(e: rusqlite::Error) -> Self {
        Self::Db(persistence_sqlite::DbError::from(e))
    }
}

impl From<crate::source_db::ReadySourceError> for AnalysisServiceError {
    fn from(error: crate::source_db::ReadySourceError) -> Self {
        match error {
            crate::source_db::ReadySourceError::Db(error) => Self::Db(error),
            crate::source_db::ReadySourceError::NotFound { data_source_id, .. } => {
                Self::NotFound("Data source", data_source_id)
            }
            crate::source_db::ReadySourceError::NotReady { .. } => {
                Self::InvalidInput(error.to_string())
            }
            crate::source_db::ReadySourceError::UnsupportedPlatform { .. } => {
                Self::Unsupported(error.to_string())
            }
        }
    }
}

impl AnalysisServiceError {
    pub(crate) fn unsupported_platform(reason: impl Into<String>) -> Self {
        Self::Unsupported(format!("data source platform: {}", reason.into()))
    }

    pub(crate) fn platform_mismatch(
        capability: &str,
        source_platform: domain::DataSourcePlatform,
        capability_platform: domain::DataSourcePlatform,
    ) -> Self {
        Self::Unsupported(format!(
            "analysis capability `{capability}` belongs to {capability_platform}, not {source_platform}"
        ))
    }
}

impl transport::ServiceErrorCategory for AnalysisServiceError {
    fn category(&self) -> transport::ErrorCategory {
        match self {
            Self::Db(_) | Self::Io(_) | Self::Read(_) => transport::ErrorCategory::Io,
            Self::Extraction(_) => transport::ErrorCategory::Parser,
            Self::NotFound(_, _) | Self::InvalidInput(_) => transport::ErrorCategory::Validation,
            Self::Unsupported(_) => transport::ErrorCategory::Unsupported,
            Self::Cancelled => transport::ErrorCategory::Timeout,
            Self::Other(_) => transport::ErrorCategory::Internal,
        }
    }

    fn recoverable(&self) -> Option<bool> {
        matches!(self, Self::Cancelled).then_some(true)
    }
}
