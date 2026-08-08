use thiserror::Error;

/// Typed error for registry parsing, recovery, and SAM decryption.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum RegistryError {
    #[error("invalid hive: {0}")]
    InvalidHive(String),

    #[error("truncated data at offset {offset}: {reason}")]
    Truncated { offset: usize, reason: String },

    #[error("invalid cell: {0}")]
    InvalidCell(String),

    #[error("UTF-16 decode error: {0}")]
    Utf16(String),

    #[error("missing registry key: {0}")]
    MissingKey(String),

    #[error("unsupported cipher or hash algorithm: {0}")]
    UnsupportedCipher(String),

    #[error("decrypt failed: {0}")]
    DecryptFailed(String),

    #[error("boot key not available")]
    MissingBootKey,

    #[error("account F record missing or unreadable")]
    MissingAccountF,

    #[error("io error: {0}")]
    Io(String),

    #[error("{0}")]
    Other(String),
}

impl From<String> for RegistryError {
    fn from(msg: String) -> Self {
        Self::Other(msg)
    }
}

impl From<&str> for RegistryError {
    fn from(msg: &str) -> Self {
        Self::Other(msg.to_owned())
    }
}

impl From<std::io::Error> for RegistryError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err.to_string())
    }
}

impl From<RegistryError> for String {
    fn from(err: RegistryError) -> Self {
        err.to_string()
    }
}

impl RegistryError {
    pub fn other<S: Into<String>>(msg: S) -> Self {
        Self::Other(msg.into())
    }

    pub fn truncated(offset: usize, reason: impl Into<String>) -> Self {
        Self::Truncated {
            offset,
            reason: reason.into(),
        }
    }

    pub fn invalid_cell<S: Into<String>>(msg: S) -> Self {
        Self::InvalidCell(msg.into())
    }

    pub fn utf16<S: Into<String>>(msg: S) -> Self {
        Self::Utf16(msg.into())
    }

    pub fn missing_key<S: Into<String>>(path: S) -> Self {
        Self::MissingKey(path.into())
    }
}
