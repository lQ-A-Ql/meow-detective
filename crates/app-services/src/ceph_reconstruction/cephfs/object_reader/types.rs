use std::path::PathBuf;

use domain::DataSourceId;
use thiserror::Error;

pub const MAX_CEPHFS_OBJECT_RANGE_LENGTH: usize = infrastructure::constants::MAX_RANGE_LENGTH;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephFsObjectSource {
    pub data_source_id: DataSourceId,
    pub inventory_id: String,
    pub source_db_path: PathBuf,
}

impl CephFsObjectSource {
    pub fn new(
        data_source_id: DataSourceId,
        inventory_id: impl Into<String>,
        source_db_path: impl Into<PathBuf>,
    ) -> Result<Self, CephFsObjectReadError> {
        let inventory_id = inventory_id.into();
        let source_db_path = source_db_path.into();
        if data_source_id.0.trim().is_empty()
            || data_source_id.0.contains('\0')
            || inventory_id.trim().is_empty()
            || inventory_id.contains('\0')
            || source_db_path.as_os_str().is_empty()
        {
            return Err(CephFsObjectReadError::InvalidSourceBinding);
        }
        Ok(Self {
            data_source_id,
            inventory_id,
            source_db_path,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct CephFsObjectReadProvenance {
    pub data_source_id: String,
    pub inventory_id: String,
    pub object_identity_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephFsObjectRange {
    pub filesystem_identity: String,
    pub locator: String,
    pub object_size: u64,
    pub offset: u64,
    pub bytes: Vec<u8>,
    pub provenance: Vec<CephFsObjectReadProvenance>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephFsObjectMetadata {
    pub filesystem_identity: String,
    pub locator: String,
    pub object_size: u64,
    pub provenance: Vec<CephFsObjectReadProvenance>,
}

pub trait CephFsObjectRangeReader {
    fn inspect_object(
        &mut self,
        locator: &super::super::CephFsObjectLocator,
    ) -> Result<CephFsObjectMetadata, CephFsObjectReadError>;

    fn read_range(
        &mut self,
        locator: &super::super::CephFsObjectLocator,
        offset: u64,
        length: usize,
    ) -> Result<CephFsObjectRange, CephFsObjectReadError>;
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum CephFsObjectReadError {
    #[error("invalid CephFS descriptor binding")]
    InvalidDescriptor,
    #[error("invalid CephFS source binding")]
    InvalidSourceBinding,
    #[error("CephFS source coverage is not closed: expected {expected}, supplied {supplied}")]
    CoverageNotClosed { expected: usize, supplied: usize },
    #[error("duplicate CephFS source binding: {data_source_id}")]
    DuplicateSource { data_source_id: String },
    #[error("duplicate CephFS inventory binding: {inventory_id}")]
    DuplicateInventory { inventory_id: String },
    #[error("CephFS locator is not bound to descriptor {filesystem_identity}")]
    LocatorMismatch { filesystem_identity: String },
    #[error("CephFS source database is unavailable for inventory {inventory_id}")]
    SourceDbUnavailable { inventory_id: String },
    #[error("CephFS metadata inventory is unavailable or incomplete: {inventory_id}")]
    InventoryUnavailable { inventory_id: String },
    #[error("CephFS object was not found: {locator}")]
    ObjectNotFound { locator: String },
    #[error(
        "CephFS object replica coverage is incomplete for {locator}: expected {expected}, present {present}"
    )]
    ReplicaCoverageIncomplete {
        locator: String,
        expected: usize,
        present: usize,
    },
    #[error("CephFS object metadata conflicts across replicas: {locator}")]
    MetadataConflict { locator: String },
    #[error("CephFS object read plan is unavailable for inventory {inventory_id}")]
    ReadPlanUnavailable { inventory_id: String },
    #[error("CephFS object device is unavailable for inventory {inventory_id}")]
    DeviceUnavailable { inventory_id: String },
    #[error("CephFS object range exceeds the {maximum} byte limit: {requested}")]
    RangeTooLarge { requested: usize, maximum: usize },
    #[error("CephFS object range overflows: {locator}")]
    RangeOverflow { locator: String },
    #[error("CephFS object range exceeds size {object_size}: {locator}")]
    RangeOutOfBounds { locator: String, object_size: u64 },
    #[error("CephFS object read failed for inventory {inventory_id}")]
    ObjectRead { inventory_id: String },
    #[error("CephFS object bytes conflict across replicas: {locator}")]
    ByteConflict { locator: String },
    #[error("CephFS object reader returned a response outside the requested binding: {locator}")]
    ResponseMismatch { locator: String },
}

impl transport::ServiceErrorCategory for CephFsObjectReadError {
    fn category(&self) -> transport::ErrorCategory {
        match self {
            Self::InvalidDescriptor
            | Self::InvalidSourceBinding
            | Self::CoverageNotClosed { .. }
            | Self::DuplicateSource { .. }
            | Self::DuplicateInventory { .. }
            | Self::LocatorMismatch { .. }
            | Self::RangeTooLarge { .. }
            | Self::RangeOutOfBounds { .. } => transport::ErrorCategory::Validation,
            Self::SourceDbUnavailable { .. }
            | Self::DeviceUnavailable { .. }
            | Self::ObjectRead { .. } => transport::ErrorCategory::Io,
            Self::InventoryUnavailable { .. }
            | Self::ObjectNotFound { .. }
            | Self::ReplicaCoverageIncomplete { .. }
            | Self::MetadataConflict { .. }
            | Self::ReadPlanUnavailable { .. }
            | Self::RangeOverflow { .. }
            | Self::ByteConflict { .. }
            | Self::ResponseMismatch { .. } => transport::ErrorCategory::Parser,
        }
    }
}
