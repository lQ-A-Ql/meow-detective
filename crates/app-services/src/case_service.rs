//! Case lifecycle service facade.
//!
//! Implementation lives in the submodules: `lifecycle` (create/delete),
//! `opening` (open/preflight), `metrics`, `close_drain`, `data_source_deletion`
//! and `platform_compatibility`. Case-wide metric aggregation routes every
//! source read through `open_ready_source_connections_read_only(...)`; no
//! submodule opens a per-source database directly.
use std::path::PathBuf;
use thiserror::Error;

mod close_drain;
mod data_source_deletion;
mod lifecycle;
mod metrics;
mod opening;
mod platform_compatibility;

pub use close_drain::{close_case_drain, DrainResult};
pub use data_source_deletion::{delete_data_source, delete_data_source_in};
pub use lifecycle::{create_case, delete_case, delete_case_in};
pub use metrics::get_case_metrics_for_case;
pub use opening::open_case;
pub use platform_compatibility::ensure_supported_data_source_platforms;

#[derive(Debug, Error)]
pub enum CaseServiceError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Database error: {0}")]
    Db(#[from] persistence_sqlite::DbError),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Case already exists at path: {0}")]
    AlreadyExists(PathBuf),
    #[error("No case found at path: {0}")]
    NotFound(PathBuf),
    #[error("Invalid case directory: {0}")]
    InvalidCaseDir(String),
    #[error("Unsupported data source platform in case: {0}")]
    UnsupportedPlatform(String),
    #[error("Data source '{data_source_id}' deletion requires recovery from case tombstone '{tombstone}': {reason}")]
    DataSourceDeleteRecoveryPending {
        data_source_id: String,
        tombstone: String,
        reason: String,
    },
    #[error("Data source '{data_source_id}' registration was deleted, but case tombstone cleanup is pending at '{tombstone}': {source}")]
    DataSourceDeleteCleanupPending {
        data_source_id: String,
        tombstone: String,
        #[source]
        source: std::io::Error,
    },
    #[error("Data source '{data_source_id}' deletion failed and rollback step '{step}' also failed at case tombstone '{tombstone}'; original error: {original}; rollback error: {rollback}")]
    DataSourceDeleteRollbackFailed {
        data_source_id: String,
        tombstone: String,
        step: &'static str,
        #[source]
        original: Box<CaseServiceError>,
        rollback: std::io::Error,
    },
}

impl From<crate::source_db::ReadySourceError> for CaseServiceError {
    fn from(error: crate::source_db::ReadySourceError) -> Self {
        match error {
            crate::source_db::ReadySourceError::Db(error) => Self::Db(error),
            crate::source_db::ReadySourceError::UnsupportedPlatform { .. } => {
                Self::UnsupportedPlatform(error.to_string())
            }
            crate::source_db::ReadySourceError::NotFound { .. }
            | crate::source_db::ReadySourceError::NotReady { .. } => {
                Self::InvalidCaseDir(error.to_string())
            }
        }
    }
}

impl transport::ServiceErrorCategory for CaseServiceError {
    fn category(&self) -> transport::ErrorCategory {
        match self {
            Self::Io(_)
            | Self::DataSourceDeleteRecoveryPending { .. }
            | Self::DataSourceDeleteCleanupPending { .. }
            | Self::DataSourceDeleteRollbackFailed { .. } => transport::ErrorCategory::Io,
            Self::Db(_) => transport::ErrorCategory::Io,
            Self::Json(_) => transport::ErrorCategory::Parser,
            Self::AlreadyExists(_) | Self::InvalidCaseDir(_) => {
                transport::ErrorCategory::Validation
            }
            Self::UnsupportedPlatform(_) => transport::ErrorCategory::Unsupported,
            Self::NotFound(_) => transport::ErrorCategory::Validation,
        }
    }

    fn code(&self) -> Option<&'static str> {
        match self {
            Self::DataSourceDeleteRecoveryPending { .. } => {
                Some("DATA_SOURCE_DELETE_RECOVERY_PENDING")
            }
            Self::DataSourceDeleteCleanupPending { .. } => {
                Some("DATA_SOURCE_DELETE_CLEANUP_PENDING")
            }
            Self::DataSourceDeleteRollbackFailed { .. } => {
                Some("DATA_SOURCE_DELETE_ROLLBACK_FAILED")
            }
            _ => None,
        }
    }

    fn user_message(&self) -> Option<&'static str> {
        match self {
            Self::DataSourceDeleteRecoveryPending { .. } => {
                Some("Data source deletion is waiting for managed-storage recovery.")
            }
            Self::DataSourceDeleteCleanupPending { .. } => Some(
                "The data source registration was deleted, but managed-storage cleanup is pending.",
            ),
            Self::DataSourceDeleteRollbackFailed { .. } => {
                Some("Data source deletion failed and rollback requires recovery.")
            }
            _ => None,
        }
    }

    fn recoverable(&self) -> Option<bool> {
        match self {
            Self::DataSourceDeleteRecoveryPending { .. }
            | Self::DataSourceDeleteCleanupPending { .. }
            | Self::DataSourceDeleteRollbackFailed { .. } => Some(true),
            _ => None,
        }
    }

    fn safe_details(&self) -> Option<serde_json::Value> {
        match self {
            Self::DataSourceDeleteRecoveryPending {
                data_source_id,
                tombstone,
                ..
            } => Some(serde_json::json!({
                "dataSourceId": data_source_id,
                "tombstone": tombstone,
                "registrationDeleted": false,
                "state": "recoveryPending"
            })),
            Self::DataSourceDeleteCleanupPending {
                data_source_id,
                tombstone,
                ..
            } => Some(serde_json::json!({
                "dataSourceId": data_source_id,
                "tombstone": tombstone,
                "registrationDeleted": true,
                "state": "cleanupPending"
            })),
            Self::DataSourceDeleteRollbackFailed {
                data_source_id,
                tombstone,
                step,
                ..
            } => Some(serde_json::json!({
                "dataSourceId": data_source_id,
                "tombstone": tombstone,
                "registrationDeleted": false,
                "state": "rollbackFailed",
                "rollbackStep": step
            })),
            _ => None,
        }
    }

    fn suggestion(&self) -> Option<&'static str> {
        match self {
            Self::DataSourceDeleteRecoveryPending { .. }
            | Self::DataSourceDeleteRollbackFailed { .. } => Some(
                "Preserve the tombstone, review backend logs and recovery state, then retry the deletion after recovery.",
            ),
            Self::DataSourceDeleteCleanupPending { .. } => Some(
                "Retry managed-storage cleanup; the data source registration has already been removed.",
            ),
            _ => None,
        }
    }
}

pub type Result<T> = std::result::Result<T, CaseServiceError>;
