use transport::{ErrorCategory, ServiceErrorCategory};

#[derive(Debug, thiserror::Error)]
pub enum BitLockerServiceError {
    #[error(transparent)]
    Database(#[from] persistence_sqlite::DbError),
    #[error(transparent)]
    Source(#[from] crate::source_db::ReadySourceError),
    #[error("data source '{data_source_id}' does not contain partition {partition_index}")]
    PartitionNotFound {
        data_source_id: String,
        partition_index: u32,
    },
    #[error("partition {partition_index} is not a BitLocker volume")]
    NotBitLocker { partition_index: u32 },
    #[error("data source kind '{kind}' cannot contain a directly readable BitLocker volume")]
    UnsupportedSourceKind { kind: String },
    #[error("BitLocker partition window is invalid: {0}")]
    InvalidWindow(#[source] std::io::Error),
    #[error("BitLocker evidence source could not be opened: {0}")]
    EvidenceOpen(#[source] std::io::Error),
    #[error(transparent)]
    Volume(#[from] volume_bitlocker::BitLockerError),
    #[error(transparent)]
    Runtime(#[from] crate::bitlocker_runtime::BitLockerRuntimeError),
    #[error("BitLocker plaintext filesystem is unsupported: {0}")]
    UnsupportedFilesystem(String),
    #[error("BitLocker catalog root state is inconsistent: {0}")]
    CatalogState(String),
    #[error("BitLocker preview reads did not drain before the lock timeout")]
    DrainTimeout,
    #[error("BitLocker preview runtime failed: {0}")]
    PreviewRuntime(#[from] crate::file_service::FileServiceError),
}

impl ServiceErrorCategory for BitLockerServiceError {
    fn category(&self) -> ErrorCategory {
        match self {
            Self::PartitionNotFound { .. } | Self::NotBitLocker { .. } => ErrorCategory::Validation,
            Self::UnsupportedSourceKind { .. } | Self::UnsupportedFilesystem(_) => {
                ErrorCategory::Unsupported
            }
            Self::InvalidWindow(_) | Self::EvidenceOpen(_) | Self::Database(_) => ErrorCategory::Io,
            Self::Volume(error) => volume_error_category(error),
            Self::Source(crate::source_db::ReadySourceError::UnsupportedPlatform { .. }) => {
                ErrorCategory::Unsupported
            }
            Self::Source(_)
            | Self::CatalogState(_)
            | Self::Runtime(_)
            | Self::PreviewRuntime(_) => ErrorCategory::Internal,
            Self::DrainTimeout => ErrorCategory::Timeout,
        }
    }

    fn code(&self) -> Option<&'static str> {
        match self {
            Self::PartitionNotFound { .. } => Some("BITLOCKER_PARTITION_NOT_FOUND"),
            Self::NotBitLocker { .. } => Some("BITLOCKER_NOT_A_VOLUME"),
            Self::UnsupportedSourceKind { .. } => Some("BITLOCKER_SOURCE_UNSUPPORTED"),
            Self::InvalidWindow(_) => Some("BITLOCKER_PARTITION_WINDOW_INVALID"),
            Self::EvidenceOpen(_) => Some("BITLOCKER_EVIDENCE_OPEN_FAILED"),
            Self::Volume(error) => Some(error.code()),
            Self::Runtime(crate::bitlocker_runtime::BitLockerRuntimeError::Locked) => {
                Some("BITLOCKER_LOCKED")
            }
            Self::UnsupportedFilesystem(_) => Some("BITLOCKER_FILESYSTEM_UNSUPPORTED"),
            Self::CatalogState(_) => Some("BITLOCKER_CATALOG_STATE_INVALID"),
            Self::DrainTimeout => Some("BITLOCKER_LOCK_TIMEOUT"),
            _ => None,
        }
    }

    fn user_message(&self) -> Option<&'static str> {
        match self {
            Self::Volume(volume_bitlocker::BitLockerError::CredentialRejected) => {
                Some("The BitLocker credential was rejected")
            }
            Self::Volume(volume_bitlocker::BitLockerError::MetadataUnreadable { .. }) => {
                Some("The BitLocker metadata could not be read reliably")
            }
            Self::Runtime(crate::bitlocker_runtime::BitLockerRuntimeError::Locked) => {
                Some("The BitLocker volume is locked")
            }
            Self::DrainTimeout => Some("Active preview reads prevented the volume from locking"),
            _ => None,
        }
    }

    fn recoverable(&self) -> Option<bool> {
        match self {
            Self::Volume(error) => Some(error.is_retryable_with_credential()),
            Self::Runtime(crate::bitlocker_runtime::BitLockerRuntimeError::Locked)
            | Self::DrainTimeout => Some(true),
            Self::UnsupportedSourceKind { .. } | Self::UnsupportedFilesystem(_) => Some(false),
            _ => None,
        }
    }
}

fn volume_error_category(error: &volume_bitlocker::BitLockerError) -> ErrorCategory {
    match error {
        volume_bitlocker::BitLockerError::CredentialRejected
        | volume_bitlocker::BitLockerError::Locked => ErrorCategory::Security,
        volume_bitlocker::BitLockerError::UnsupportedEncryptionMethod { .. }
        | volume_bitlocker::BitLockerError::UnsupportedProtector { .. } => {
            ErrorCategory::Unsupported
        }
        volume_bitlocker::BitLockerError::MetadataUnreadable { .. } => ErrorCategory::Parser,
        volume_bitlocker::BitLockerError::EvidenceRead { .. }
        | volume_bitlocker::BitLockerError::OutOfBounds { .. } => ErrorCategory::Io,
    }
}
