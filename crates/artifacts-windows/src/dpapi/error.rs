use thiserror::Error;

/// Errors returned by the byte-oriented Windows DPAPI implementation.
///
/// Error messages intentionally describe the format or algorithm only. They
/// never include keys, decrypted values, or evidence paths.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DpapiError {
    #[error("DPAPI input is too short: need {needed} bytes, got {actual}")]
    TooShort { needed: usize, actual: usize },
    #[error("invalid DPAPI format: {0}")]
    InvalidFormat(&'static str),
    #[error("unsupported DPAPI version {0}")]
    UnsupportedVersion(u32),
    #[error("unsupported DPAPI algorithm {0:#x}")]
    UnsupportedAlgorithm(u32),
    #[error("invalid DPAPI key or IV length")]
    InvalidKeyLength,
    #[error("DPAPI decryption failed")]
    DecryptionFailed,
    #[error("DPAPI integrity verification failed")]
    IntegrityMismatch,
    #[error("no matching DPAPI master key was found")]
    NoMatchingMasterKey,
    #[error("invalid Chromium Local State encrypted key")]
    InvalidLocalStateKey,
    #[error("invalid Chromium encrypted value")]
    InvalidChromiumValue,
}
