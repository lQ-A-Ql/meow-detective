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
    #[error(transparent)]
    KeyStore(#[from] super::BitLockerKeyStoreError),
    #[error("BitLocker plaintext filesystem is unsupported: {0}")]
    UnsupportedFilesystem(String),
    #[error("BitLocker plaintext filesystem validation failed: {0}")]
    PlaintextValidation(#[source] std::io::Error),
    #[error("BitLocker catalog root state is inconsistent: {0}")]
    CatalogState(String),
    #[error("BitLocker preview reads did not drain before the lock timeout")]
    DrainTimeout,
    #[error("no persisted key package exists for this BitLocker volume")]
    StoredKeyNotFound,
    #[error("the persisted BitLocker key does not match the current volume metadata")]
    PersistedKeyFingerprintMismatch,
    #[error("BitLocker preview runtime failed: {0}")]
    PreviewRuntime(#[from] crate::file_service::FileServiceError),
    #[error(transparent)]
    MemoryImage(#[from] memory_windows::MemoryWindowsError),
    #[error("the structurally recovered BitLocker VMK did not pass volume-bound validation")]
    MemoryKeyNotValidated,
}

impl ServiceErrorCategory for BitLockerServiceError {
    fn category(&self) -> ErrorCategory {
        match self {
            Self::PartitionNotFound { .. }
            | Self::NotBitLocker { .. }
            | Self::PersistedKeyFingerprintMismatch => ErrorCategory::Validation,
            Self::UnsupportedSourceKind { .. } | Self::UnsupportedFilesystem(_) => {
                ErrorCategory::Unsupported
            }
            Self::InvalidWindow(_) | Self::EvidenceOpen(_) | Self::Database(_) => ErrorCategory::Io,
            Self::PlaintextValidation(_) => ErrorCategory::Parser,
            Self::Volume(error) => volume_error_category(error),
            Self::KeyStore(super::BitLockerKeyStoreError::Unsupported) => {
                ErrorCategory::Unsupported
            }
            Self::KeyStore(super::BitLockerKeyStoreError::CorruptBlob(error)) => {
                volume_error_category(error)
            }
            Self::KeyStore(super::BitLockerKeyStoreError::Platform { .. }) => {
                ErrorCategory::External
            }
            Self::Source(crate::source_db::ReadySourceError::UnsupportedPlatform { .. }) => {
                ErrorCategory::Unsupported
            }
            Self::Source(_)
            | Self::CatalogState(_)
            | Self::Runtime(_)
            | Self::PreviewRuntime(_) => ErrorCategory::Internal,
            Self::MemoryImage(memory_windows::MemoryWindowsError::PhysicalRead { .. }) => {
                ErrorCategory::Io
            }
            Self::MemoryImage(error) if is_unsupported_memory_profile(error) => {
                ErrorCategory::Unsupported
            }
            Self::MemoryImage(_) => ErrorCategory::Parser,
            Self::MemoryKeyNotValidated => ErrorCategory::Security,
            Self::DrainTimeout => ErrorCategory::Timeout,
            Self::StoredKeyNotFound => ErrorCategory::Validation,
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
            Self::KeyStore(super::BitLockerKeyStoreError::Unsupported) => {
                Some("BITLOCKER_KEY_STORE_UNSUPPORTED")
            }
            Self::KeyStore(super::BitLockerKeyStoreError::Platform { .. }) => {
                Some("BITLOCKER_KEY_STORE_FAILED")
            }
            Self::KeyStore(super::BitLockerKeyStoreError::CorruptBlob(error)) => Some(error.code()),
            Self::UnsupportedFilesystem(_) => Some("BITLOCKER_FILESYSTEM_UNSUPPORTED"),
            Self::PlaintextValidation(_) => Some("BITLOCKER_PLAINTEXT_VALIDATION_FAILED"),
            Self::CatalogState(_) => Some("BITLOCKER_CATALOG_STATE_INVALID"),
            Self::DrainTimeout => Some("BITLOCKER_LOCK_TIMEOUT"),
            Self::StoredKeyNotFound => Some("BITLOCKER_STORED_KEY_NOT_FOUND"),
            Self::PersistedKeyFingerprintMismatch => {
                Some("BITLOCKER_PERSISTED_KEY_FINGERPRINT_MISMATCH")
            }
            Self::MemoryImage(error) if is_unsupported_memory_profile(error) => {
                Some("BITLOCKER_MEMORY_PROFILE_UNSUPPORTED")
            }
            Self::MemoryImage(_) => Some("BITLOCKER_MEMORY_IMAGE_INVALID"),
            Self::MemoryKeyNotValidated => Some("BITLOCKER_MEMORY_KEY_NOT_VALIDATED"),
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
            Self::StoredKeyNotFound => Some("No saved key is available for this BitLocker volume"),
            Self::PersistedKeyFingerprintMismatch => {
                Some("The saved BitLocker key belongs to different volume metadata")
            }
            Self::KeyStore(super::BitLockerKeyStoreError::Unsupported) => {
                Some("Secure BitLocker key storage is unavailable on this platform")
            }
            Self::KeyStore(super::BitLockerKeyStoreError::Platform { .. }) => {
                Some("Windows Credential Manager could not complete the BitLocker key operation")
            }
            Self::MemoryKeyNotValidated => {
                Some("The memory-recovered BitLocker key could not be validated against the volume")
            }
            Self::MemoryImage(error) if is_unsupported_memory_profile(error) => Some(
                "This Windows memory build does not have a reviewed BitLocker recovery profile",
            ),
            Self::KeyStore(super::BitLockerKeyStoreError::CorruptBlob(_)) => {
                Some("The stored BitLocker key package is invalid")
            }
            _ => None,
        }
    }

    fn recoverable(&self) -> Option<bool> {
        match self {
            Self::Volume(error) => Some(error.is_retryable_with_credential()),
            Self::Runtime(crate::bitlocker_runtime::BitLockerRuntimeError::Locked)
            | Self::DrainTimeout => Some(true),
            Self::StoredKeyNotFound | Self::PersistedKeyFingerprintMismatch => Some(false),
            Self::UnsupportedSourceKind { .. } | Self::UnsupportedFilesystem(_) => Some(false),
            Self::KeyStore(super::BitLockerKeyStoreError::Platform { .. }) => Some(true),
            Self::KeyStore(super::BitLockerKeyStoreError::Unsupported)
            | Self::KeyStore(super::BitLockerKeyStoreError::CorruptBlob(_)) => Some(false),
            Self::MemoryKeyNotValidated => Some(true),
            Self::MemoryImage(error) if is_unsupported_memory_profile(error) => Some(false),
            _ => None,
        }
    }

    fn suggestion(&self) -> Option<&'static str> {
        match self {
            Self::MemoryKeyNotValidated => Some(
                "Select a raw Windows memory image from the same system, captured while the BitLocker volume was unlocked",
            ),
            _ => None,
        }
    }
}

fn is_unsupported_memory_profile(error: &memory_windows::MemoryWindowsError) -> bool {
    matches!(
        error,
        memory_windows::MemoryWindowsError::TargetedKernelCodeViewMismatch
            | memory_windows::MemoryWindowsError::UnsupportedBitLockerMemoryProfile
    )
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
        volume_bitlocker::BitLockerError::PersistedKeyInvalid { .. }
        | volume_bitlocker::BitLockerError::PersistedKeyMismatch => ErrorCategory::Security,
        volume_bitlocker::BitLockerError::EvidenceRead { .. }
        | volume_bitlocker::BitLockerError::OutOfBounds { .. } => ErrorCategory::Io,
    }
}
