//! Error classification for the BitLocker volume layer.
//!
//! Every variant carries a stable machine code so the service layer can map it
//! into `transport::errors::ApiErrorDto` without string matching.
//!
//! # Why no credential ever appears here
//!
//! Error details cross into logs, events, and reports. A variant that carried a
//! password — even in a "wrong password" message — would leak it into all three.
//! No variant in this enum holds credential material, and none may be added.

use thiserror::Error;

/// Result alias for the BitLocker volume layer.
pub type Result<T> = std::result::Result<T, BitLockerError>;

/// A failure in BitLocker metadata parsing, key derivation, or volume reading.
#[derive(Debug, Error)]
pub enum BitLockerError {
    /// The volume is encrypted and no verified key is registered for it.
    #[error("volume is locked")]
    Locked,

    /// The supplied credential did not unwrap the volume master key.
    ///
    /// Deliberately carries nothing: neither the credential nor which protector
    /// it was tried against, because both narrow a later guess.
    #[error("credential did not unlock the volume")]
    CredentialRejected,

    /// The volume's cipher is recognized but not decryptable in this build.
    #[error("unsupported encryption method: {label} ({code:#06X})")]
    UnsupportedEncryptionMethod { code: u16, label: &'static str },

    /// No protector on the volume can be unlocked by this build.
    #[error("no supported key protector: volume carries only {found}")]
    UnsupportedProtector { found: String },

    /// The FVE metadata could not be parsed, in all available copies.
    #[error("FVE metadata is unreadable: {reason}")]
    MetadataUnreadable { reason: String },

    /// A key package loaded from persistent storage is malformed or corrupt.
    #[error("persisted BitLocker key package is invalid: {reason}")]
    PersistedKeyInvalid { reason: &'static str },

    /// A stored key package belongs to different FVE metadata.
    #[error("persisted BitLocker key package does not match this volume")]
    PersistedKeyMismatch,

    /// A read against the underlying evidence reader failed.
    #[error("evidence read failed at volume offset {offset}: {source}")]
    EvidenceRead {
        offset: u64,
        #[source]
        source: std::io::Error,
    },

    /// A read was requested outside the decrypted volume's bounds.
    #[error("read of {length} bytes at offset {offset} exceeds volume length {volume_length}")]
    OutOfBounds {
        offset: u64,
        length: u64,
        volume_length: u64,
    },
}

impl BitLockerError {
    /// A stable machine code for the service and transport layers.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::Locked => "BITLOCKER_LOCKED",
            Self::CredentialRejected => "BITLOCKER_CREDENTIAL_REJECTED",
            Self::UnsupportedEncryptionMethod { .. } => "BITLOCKER_UNSUPPORTED_METHOD",
            Self::UnsupportedProtector { .. } => "BITLOCKER_UNSUPPORTED_PROTECTOR",
            Self::MetadataUnreadable { .. } => "BITLOCKER_METADATA_UNREADABLE",
            Self::PersistedKeyInvalid { .. } => "BITLOCKER_STORED_KEY_INVALID",
            Self::PersistedKeyMismatch => "BITLOCKER_STORED_KEY_MISMATCH",
            Self::EvidenceRead { .. } => "BITLOCKER_EVIDENCE_READ_FAILED",
            Self::OutOfBounds { .. } => "BITLOCKER_OUT_OF_BOUNDS",
        }
    }

    /// Whether supplying a different credential could change the outcome.
    ///
    /// Drives the frontend's retry affordance: a rejected password is worth
    /// retrying, an unsupported cipher is not.
    #[must_use]
    pub fn is_retryable_with_credential(&self) -> bool {
        matches!(self, Self::Locked | Self::CredentialRejected)
    }
}

#[cfg(test)]
#[path = "../tests/unit/error.rs"]
mod tests;
