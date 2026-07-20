use thiserror::Error;

use super::super::CephFsObjectReadError;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum CephFsFileDataReadError {
    #[error("invalid CephFS file data descriptor: {0}")]
    InvalidDescriptor(&'static str),
    #[error("invalid CephFS sparse extent proof: {0}")]
    InvalidSparseExtentProof(&'static str),
    #[error("CephFS file range exceeds the {maximum} byte limit: {requested}")]
    RangeTooLarge { requested: usize, maximum: usize },
    #[error("CephFS file range arithmetic overflow")]
    RangeOverflow,
    #[error("CephFS file range {offset:#x}~{length:#x} exceeds size {file_size:#x}")]
    RangeOutOfBounds {
        offset: u64,
        length: u64,
        file_size: u64,
    },
    #[error("CephFS file layout cannot map the requested range")]
    InvalidLayout,
    #[error("CephFS data object locator is invalid")]
    InvalidLocator,
    #[error("CephFS data object reader returned an inconsistent response: {locator}")]
    ResponseMismatch { locator: String },
    #[error(transparent)]
    Object(#[from] CephFsObjectReadError),
}

impl transport::ServiceErrorCategory for CephFsFileDataReadError {
    fn category(&self) -> transport::ErrorCategory {
        match self {
            Self::InvalidDescriptor(_)
            | Self::InvalidSparseExtentProof(_)
            | Self::RangeTooLarge { .. }
            | Self::RangeOutOfBounds { .. } => transport::ErrorCategory::Validation,
            Self::Object(error) => transport::ServiceErrorCategory::category(error),
            Self::RangeOverflow
            | Self::InvalidLayout
            | Self::InvalidLocator
            | Self::ResponseMismatch { .. } => transport::ErrorCategory::Parser,
        }
    }
}
