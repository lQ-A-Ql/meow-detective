use std::fmt;
use std::io;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XfsLogIssueKind {
    ExternalLogUnsupported,
    InvalidGeometry,
    InvalidRecord,
    TruncatedRecord,
    CycleMismatch,
    ChecksumMismatch,
    InvalidOperation,
    DeletionEvidenceUnavailable,
    LimitReached,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XfsLogIssue {
    pub kind: XfsLogIssueKind,
    pub log_block: Option<u64>,
    pub message: String,
}

impl XfsLogIssue {
    pub(crate) fn new(
        kind: XfsLogIssueKind,
        log_block: Option<u64>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            log_block,
            message: message.into(),
        }
    }
}

impl fmt::Display for XfsLogIssue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.log_block {
            Some(block) => write!(formatter, "{} (log block {block})", self.message),
            None => formatter.write_str(&self.message),
        }
    }
}

#[derive(Debug, Error)]
pub enum XfsLogError {
    #[error("unsupported XFS log layout: {0}")]
    Unsupported(XfsLogIssue),
    #[error("invalid XFS log geometry: {0}")]
    InvalidGeometry(String),
    #[error("invalid XFS log data: {0}")]
    InvalidData(String),
    #[error("XFS log I/O failed: {0}")]
    Io(#[from] io::Error),
}
