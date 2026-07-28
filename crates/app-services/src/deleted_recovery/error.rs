use thiserror::Error;
use transport::{ErrorCategory, ServiceErrorCategory};

#[derive(Debug, Error)]
pub enum DeletedRecoveryError {
    #[error("database error: {0}")]
    Database(#[from] persistence_sqlite::DbError),
    #[error("source routing error: {0}")]
    Source(#[from] crate::source_db::ReadySourceError),
    #[error("deleted recovery scan was not found for data source '{data_source_id}' partition {partition_index}")]
    NotFound {
        data_source_id: String,
        partition_index: u32,
    },
    #[error("deleted recovery '{recovery_id}' was not found for data source '{data_source_id}'")]
    RecoveryNotFound {
        data_source_id: String,
        recovery_id: String,
    },
    #[error("deleted recovery content is unavailable: {0}")]
    ContentUnavailable(String),
    #[error("deleted recovery range is invalid: {0}")]
    InvalidRange(String),
    #[error("deleted recovery integrity verification failed: {0}")]
    Integrity(String),
    #[error("unsupported recovery target: {0}")]
    Unsupported(String),
    #[error("BitLocker volume is locked; unlock it before deleted-file recovery")]
    BitLockerLocked,
    #[error("invalid recovery state: {0}")]
    InvalidState(String),
    #[error("recovery parser error: {0}")]
    Parser(String),
    #[error("recovery I/O error: {0}")]
    Io(#[from] std::io::Error),
}

impl ServiceErrorCategory for DeletedRecoveryError {
    fn category(&self) -> ErrorCategory {
        match self {
            Self::Database(_) | Self::Io(_) => ErrorCategory::Io,
            Self::Source(crate::source_db::ReadySourceError::UnsupportedPlatform { .. })
            | Self::Unsupported(_)
            | Self::BitLockerLocked
            | Self::ContentUnavailable(_) => ErrorCategory::Unsupported,
            Self::Source(crate::source_db::ReadySourceError::NotFound { .. })
            | Self::Source(crate::source_db::ReadySourceError::NotReady { .. })
            | Self::NotFound { .. }
            | Self::RecoveryNotFound { .. }
            | Self::InvalidRange(_) => ErrorCategory::Validation,
            Self::Integrity(_) => ErrorCategory::Security,
            Self::Parser(_) => ErrorCategory::Parser,
            Self::Source(crate::source_db::ReadySourceError::Db(_)) | Self::InvalidState(_) => {
                ErrorCategory::Internal
            }
        }
    }

    fn code(&self) -> Option<&'static str> {
        Some(match self {
            Self::Database(_) => "RECOVERY_DATABASE_ERROR",
            Self::Source(_) => "RECOVERY_SOURCE_ERROR",
            Self::NotFound { .. } => "RECOVERY_SCAN_NOT_FOUND",
            Self::RecoveryNotFound { .. } => "RECOVERY_NOT_FOUND",
            Self::ContentUnavailable(_) => "RECOVERY_CONTENT_UNAVAILABLE",
            Self::InvalidRange(_) => "RECOVERY_RANGE_INVALID",
            Self::Integrity(_) => "RECOVERY_INTEGRITY_MISMATCH",
            Self::Unsupported(_) => "RECOVERY_UNSUPPORTED",
            Self::BitLockerLocked => "RECOVERY_BITLOCKER_LOCKED",
            Self::InvalidState(_) => "RECOVERY_INVALID_STATE",
            Self::Parser(_) => "RECOVERY_PARSER_ERROR",
            Self::Io(_) => "RECOVERY_IO_ERROR",
        })
    }

    fn recoverable(&self) -> Option<bool> {
        Some(matches!(
            self,
            Self::NotFound { .. }
                | Self::RecoveryNotFound { .. }
                | Self::Unsupported(_)
                | Self::BitLockerLocked
                | Self::ContentUnavailable(_)
                | Self::InvalidRange(_)
                | Self::Parser(_)
        ))
    }
}
