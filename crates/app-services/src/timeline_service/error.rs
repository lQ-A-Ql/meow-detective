use thiserror::Error;

#[derive(Debug, Error)]
pub enum TimelineServiceError {
    #[error("database error: {0}")]
    Db(#[from] persistence_sqlite::DbError),
    #[error("source storage error: {0}")]
    Storage(#[from] persistence_source::fjall::FjallStorageError),
    #[error(transparent)]
    Resources(#[from] source_kernel::resource_budget::ResourceError),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("unsupported: {0}")]
    Unsupported(String),
    #[error("timeline projection cancelled")]
    Cancelled,
    #[error("timeline projection for source {source_id} is not ready: {status}")]
    ProjectionNotReady {
        source_id: String,
        status: String,
        reason: Option<String>,
    },
    #[error("timeline projection rebuild is already in progress for {source_id:?}")]
    RebuildInProgress { source_id: Option<String> },
    #[error("{0}")]
    Other(String),
}

impl transport::ServiceErrorCategory for TimelineServiceError {
    fn category(&self) -> transport::ErrorCategory {
        match self {
            Self::Db(_) | Self::Storage(_) | Self::ProjectionNotReady { .. } => {
                transport::ErrorCategory::Io
            }
            Self::NotFound(_) | Self::InvalidInput(_) | Self::RebuildInProgress { .. } => {
                transport::ErrorCategory::Validation
            }
            Self::Unsupported(_) => transport::ErrorCategory::Unsupported,
            Self::Cancelled => transport::ErrorCategory::Cancelled,
            Self::Other(_) => transport::ErrorCategory::Internal,
            Self::Resources(error) => transport::ServiceErrorCategory::category(error),
        }
    }

    fn code(&self) -> Option<&'static str> {
        match self {
            Self::ProjectionNotReady { .. } => Some("TIMELINE_PROJECTION_NOT_READY"),
            Self::RebuildInProgress { .. } => Some("TIMELINE_PROJECTION_REBUILD_IN_PROGRESS"),
            _ => None,
        }
    }

    fn recoverable(&self) -> Option<bool> {
        match self {
            Self::Cancelled | Self::ProjectionNotReady { .. } | Self::RebuildInProgress { .. } => {
                Some(true)
            }
            _ => None,
        }
    }

    fn safe_details(&self) -> Option<serde_json::Value> {
        match self {
            Self::ProjectionNotReady {
                source_id,
                status,
                reason,
            } => {
                let mut map = serde_json::Map::new();
                map.insert(
                    "sourceId".to_owned(),
                    serde_json::Value::String(source_id.clone()),
                );
                map.insert(
                    "status".to_owned(),
                    serde_json::Value::String(status.clone()),
                );
                if let Some(reason) = reason {
                    map.insert(
                        "reason".to_owned(),
                        serde_json::Value::String(reason.clone()),
                    );
                }
                Some(serde_json::Value::Object(map))
            }
            Self::RebuildInProgress { source_id } => {
                let mut map = serde_json::Map::new();
                if let Some(source_id) = source_id {
                    map.insert(
                        "sourceId".to_owned(),
                        serde_json::Value::String(source_id.clone()),
                    );
                }
                Some(serde_json::Value::Object(map))
            }
            _ => None,
        }
    }
}

impl From<rusqlite::Error> for TimelineServiceError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Db(persistence_sqlite::DbError::from(error))
    }
}

impl From<timeline::TimelineProjectionPolicyError> for TimelineServiceError {
    fn from(error: timeline::TimelineProjectionPolicyError) -> Self {
        Self::InvalidInput(error.to_string())
    }
}

impl From<crate::source_db::ReadySourceError> for TimelineServiceError {
    fn from(error: crate::source_db::ReadySourceError) -> Self {
        match error {
            crate::source_db::ReadySourceError::Db(error) => Self::Db(error),
            crate::source_db::ReadySourceError::NotFound { .. } => {
                Self::NotFound(error.to_string())
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

impl From<crate::storage_runtime::StorageRuntimeError> for TimelineServiceError {
    fn from(error: crate::storage_runtime::StorageRuntimeError) -> Self {
        match error {
            crate::storage_runtime::StorageRuntimeError::TimelineProjectionNotReady {
                source_id,
                status,
                reason,
            } => Self::ProjectionNotReady {
                source_id,
                status,
                reason,
            },
            other => Self::Other(other.to_string()),
        }
    }
}

impl From<import_core::timeline_projection::TimelineProjectionError> for TimelineServiceError {
    fn from(error: import_core::timeline_projection::TimelineProjectionError) -> Self {
        use import_core::timeline_projection::TimelineProjectionError;
        match error {
            TimelineProjectionError::Cancelled => Self::Cancelled,
            TimelineProjectionError::InvalidInput(message) => Self::InvalidInput(message),
            TimelineProjectionError::WorkspaceLimit { .. } => Self::InvalidInput(error.to_string()),
            TimelineProjectionError::Db(error) => Self::Db(error),
            TimelineProjectionError::Storage(error) => Self::Storage(error),
            TimelineProjectionError::Other(message) => Self::Other(message),
        }
    }
}
