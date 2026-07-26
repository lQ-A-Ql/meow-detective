use thiserror::Error;

/// Whether file content is known to be clear, encrypted, or not yet classified.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FileEncryptionStatus {
    Unknown,
    Clear,
    Encrypted,
}

impl FileEncryptionStatus {
    pub const fn from_database_value(value: Option<i64>) -> Result<Self, InvalidEncryptionStatus> {
        match value {
            None => Ok(Self::Unknown),
            Some(0) => Ok(Self::Clear),
            Some(1) => Ok(Self::Encrypted),
            Some(value) => Err(InvalidEncryptionStatus(value)),
        }
    }

    pub const fn database_value(self) -> Option<i64> {
        match self {
            Self::Unknown => None,
            Self::Clear => Some(0),
            Self::Encrypted => Some(1),
        }
    }

    pub const fn content_is_readable(self) -> bool {
        matches!(self, Self::Clear)
    }

    /// Conservative projection for callers that still expose the legacy bool.
    pub const fn blocks_content(self) -> bool {
        !self.content_is_readable()
    }
}

impl From<bool> for FileEncryptionStatus {
    fn from(encrypted: bool) -> Self {
        if encrypted {
            Self::Encrypted
        } else {
            Self::Clear
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error("invalid file encryption status value {0}; expected NULL, 0, or 1")]
pub struct InvalidEncryptionStatus(pub i64);
