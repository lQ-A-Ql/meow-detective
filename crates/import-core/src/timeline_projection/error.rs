#[derive(Debug, thiserror::Error)]
pub enum TimelineProjectionError {
    #[error("database error: {0}")]
    Db(#[from] persistence_sqlite::DbError),
    #[error("source storage error: {0}")]
    Storage(#[from] persistence_source::fjall::FjallStorageError),
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("timeline projection cancelled")]
    Cancelled,
    #[error("timeline projection needs an estimated {required_bytes} bytes; configured workspace is {available_bytes} bytes")]
    WorkspaceLimit {
        required_bytes: u64,
        available_bytes: u64,
        identity: String,
    },
    #[error("{0}")]
    Other(String),
}

impl From<timeline::TimelineProjectionPolicyError> for TimelineProjectionError {
    fn from(error: timeline::TimelineProjectionPolicyError) -> Self {
        Self::InvalidInput(error.to_string())
    }
}

impl From<rusqlite::Error> for TimelineProjectionError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Db(persistence_sqlite::DbError::from(error))
    }
}

impl transport::ServiceErrorCategory for TimelineProjectionError {
    fn category(&self) -> transport::ErrorCategory {
        match self {
            Self::Db(_) | Self::Storage(_) => transport::ErrorCategory::Io,
            Self::InvalidInput(_) | Self::WorkspaceLimit { .. } => {
                transport::ErrorCategory::Validation
            }
            Self::Cancelled => transport::ErrorCategory::Cancelled,
            Self::Other(_) => transport::ErrorCategory::Internal,
        }
    }

    fn recoverable(&self) -> Option<bool> {
        matches!(self, Self::Cancelled).then_some(true)
    }

    fn code(&self) -> Option<&'static str> {
        matches!(self, Self::WorkspaceLimit { .. }).then_some("TIMELINE_WORKSPACE_LIMIT")
    }

    fn safe_details(&self) -> Option<serde_json::Value> {
        match self {
            Self::WorkspaceLimit {
                required_bytes,
                available_bytes,
                identity,
            } => {
                let mut details = serde_json::json!({
                    "requiredBytes": required_bytes, "availableBytes": available_bytes,
                    "estimateKind": "projectionWorkspace"
                });
                if !identity.is_empty() {
                    details["identity"] = serde_json::Value::String(identity.clone());
                }
                Some(details)
            }
            _ => None,
        }
    }
}
