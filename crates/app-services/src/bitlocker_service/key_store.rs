use std::fmt;

use volume_bitlocker::{BitLockerError, MetadataFingerprint, PersistedKeyBlob};

/// Platform-neutral storage boundary for verified BitLocker key packages.
/// Implementations must use an OS-protected secret store, never SQLite or a
/// case-workspace file.
pub trait BitLockerKeyStore: Send + Sync {
    fn load(
        &self,
        fingerprint: &MetadataFingerprint,
    ) -> Result<Option<PersistedKeyBlob>, BitLockerKeyStoreError>;

    fn store(
        &self,
        fingerprint: &MetadataFingerprint,
        blob: PersistedKeyBlob,
    ) -> Result<(), BitLockerKeyStoreError>;

    /// Deletes the key package, returning whether an entry existed.
    fn delete(&self, fingerprint: &MetadataFingerprint) -> Result<bool, BitLockerKeyStoreError>;

    fn contains(&self, fingerprint: &MetadataFingerprint) -> Result<bool, BitLockerKeyStoreError> {
        Ok(self.load(fingerprint)?.is_some())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BitLockerKeyStoreOperation {
    Load,
    Store,
    Delete,
}

impl fmt::Display for BitLockerKeyStoreOperation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Load => "load",
            Self::Store => "store",
            Self::Delete => "delete",
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum BitLockerKeyStoreError {
    #[error("BitLocker key persistence is unsupported on this platform")]
    Unsupported,
    #[error("BitLocker key store {operation} failed with system code {system_code}")]
    Platform {
        operation: BitLockerKeyStoreOperation,
        system_code: i32,
    },
    #[error("stored BitLocker key package is corrupt")]
    CorruptBlob(#[source] BitLockerError),
}
