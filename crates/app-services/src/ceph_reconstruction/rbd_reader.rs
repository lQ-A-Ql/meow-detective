use std::io::{self, Read, Seek, SeekFrom};
use std::path::PathBuf;

use ceph_wire::{CephWireError, RbdHeadImageLayout};
use evidence_core::{EvidenceReader, ReaderInfo};
use thiserror::Error;

use super::rbd_provider::{
    RbdObjectProvider, RbdObjectProviderError, RbdObjectReadOutcome, RbdObjectReadRequest,
};

pub const RBD_FEATURE_LAYERING: u64 = 1 << 0;
pub const RBD_FEATURE_STRIPINGV2: u64 = 1 << 1;
pub const RBD_FEATURE_EXCLUSIVE_LOCK: u64 = 1 << 2;
pub const RBD_FEATURE_OBJECT_MAP: u64 = 1 << 3;
pub const RBD_FEATURE_FAST_DIFF: u64 = 1 << 4;
pub const RBD_FEATURE_DEEP_FLATTEN: u64 = 1 << 5;
pub const RBD_FEATURE_JOURNALING: u64 = 1 << 6;
pub const RBD_FEATURE_DATA_POOL: u64 = 1 << 7;
pub const RBD_FEATURE_OPERATIONS: u64 = 1 << 8;
pub const RBD_FEATURE_MIGRATING: u64 = 1 << 9;
pub const RBD_FEATURE_NON_PRIMARY: u64 = 1 << 10;
pub const RBD_FEATURE_DIRTY_CACHE: u64 = 1 << 11;
pub const RBD_FEATURES_ALL: u64 = (1 << 12) - 1;

pub const RBD_OPERATION_FEATURE_CLONE_PARENT: u64 = 1 << 0;
pub const RBD_OPERATION_FEATURE_CLONE_CHILD: u64 = 1 << 1;
pub const RBD_OPERATION_FEATURE_GROUP: u64 = 1 << 2;
pub const RBD_OPERATION_FEATURE_SNAP_TRASH: u64 = 1 << 3;
pub const RBD_READ_GRANULARITY_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RbdReadContext {
    pub operation_features: u64,
    pub has_parent: bool,
    pub snapshot_id: Option<u64>,
    pub encrypted: bool,
}

#[derive(Debug, Error)]
pub enum RbdReadError {
    #[error("invalid RBD image layout: {0}")]
    InvalidLayout(#[from] CephWireError),
    #[error(
        "unsupported RBD image features: features=0x{features:016x}, unsupported=0x{unsupported:016x}"
    )]
    UnsupportedFeatures { features: u64, unsupported: u64 },
    #[error("RBD journaling is unsupported by the bounded head reader")]
    JournalingUnsupported,
    #[error("RBD parent or clone semantics are unsupported by the bounded head reader")]
    ParentCloneUnsupported,
    #[error("RBD snapshot reads are unsupported by the bounded head reader")]
    SnapshotUnsupported,
    #[error("encrypted RBD images are unsupported by the bounded head reader")]
    EncryptionUnsupported,
    #[error(
        "RBD operation features are unsupported by the bounded head reader: 0x{features:016x}"
    )]
    OperationFeaturesUnsupported { features: u64 },
    #[error("RBD image range starts beyond EOF: offset={offset}, image_size={image_size}")]
    RangeOutOfBounds { offset: u64, image_size: u64 },
    #[error(transparent)]
    Provider(#[from] RbdObjectProviderError),
    #[error("RBD provider returned the wrong object: expected={expected}, actual={actual}")]
    ObjectIdentityMismatch { expected: String, actual: String },
    #[error(
        "RBD provider returned a short object range for {object_identity}: expected={expected}, actual={actual}"
    )]
    ShortObjectRead {
        object_identity: String,
        expected: usize,
        actual: usize,
    },
    #[error("RBD range arithmetic overflow")]
    RangeOverflow,
}

pub struct RbdEvidenceReader {
    provider: Box<dyn RbdObjectProvider>,
    layout: RbdHeadImageLayout,
    info: ReaderInfo,
    position: u64,
}

impl RbdEvidenceReader {
    pub fn new(
        provider: Box<dyn RbdObjectProvider>,
        layout: RbdHeadImageLayout,
        context: RbdReadContext,
    ) -> Result<Self, RbdReadError> {
        let layout = RbdHeadImageLayout::new_with_features(
            layout.image_size,
            layout.order,
            layout.features,
            layout.object_prefix,
            layout.stripe_unit,
            layout.stripe_count,
        )?;
        validate_features(layout.features)?;
        validate_context(&context)?;
        let info = ReaderInfo {
            path: PathBuf::from("ceph-rbd-image"),
            size: layout.image_size,
            kind: "ceph-rbd-image".to_string(),
        };
        Ok(Self {
            provider,
            layout,
            info,
            position: 0,
        })
    }

    pub fn read_at(&mut self, offset: u64, output: &mut [u8]) -> Result<usize, RbdReadError> {
        if output.is_empty() || offset == self.layout.image_size {
            return Ok(0);
        }
        if offset > self.layout.image_size {
            return Err(RbdReadError::RangeOutOfBounds {
                offset,
                image_size: self.layout.image_size,
            });
        }

        let available = self.layout.image_size - offset;
        let length = usize::try_from(available.min(output.len() as u64))
            .map_err(|_| RbdReadError::RangeOverflow)?;
        let output = &mut output[..length];
        output.fill(0);

        for plan in self.layout.plan_range(offset, length as u64)? {
            let destination = usize::try_from(plan.destination_offset)
                .map_err(|_| RbdReadError::RangeOverflow)?;
            let plan_length =
                usize::try_from(plan.length).map_err(|_| RbdReadError::RangeOverflow)?;
            let end = destination
                .checked_add(plan_length)
                .ok_or(RbdReadError::RangeOverflow)?;
            let object_identity = plan.data_object_name(&self.layout.object_prefix)?;
            let request = RbdObjectReadRequest {
                object_no: plan.object_no,
                object_identity: object_identity.clone(),
                object_offset: plan.object_offset,
                length: plan_length,
            };
            let destination = output
                .get_mut(destination..end)
                .ok_or(RbdReadError::RangeOverflow)?;
            match self.provider.read_object_range(&request, destination)? {
                RbdObjectReadOutcome::Present {
                    object_identity: actual,
                    bytes_read,
                } => {
                    if actual != object_identity {
                        return Err(RbdReadError::ObjectIdentityMismatch {
                            expected: object_identity,
                            actual,
                        });
                    }
                    if bytes_read != plan_length {
                        return Err(RbdReadError::ShortObjectRead {
                            object_identity,
                            expected: plan_length,
                            actual: bytes_read,
                        });
                    }
                }
                RbdObjectReadOutcome::Missing => destination.fill(0),
            }
        }
        Ok(length)
    }
}

impl Read for RbdEvidenceReader {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        let read = self
            .read_at(self.position, output)
            .map_err(RbdReadError::into_io_error)?;
        self.position = self
            .position
            .checked_add(read as u64)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "RBD position overflow"))?;
        Ok(read)
    }
}

impl Seek for RbdEvidenceReader {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        let next = match position {
            SeekFrom::Start(value) => i128::from(value),
            SeekFrom::End(value) => i128::from(self.layout.image_size) + i128::from(value),
            SeekFrom::Current(value) => i128::from(self.position) + i128::from(value),
        };
        if !(0..=i128::from(self.layout.image_size)).contains(&next) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "RBD image seek lies outside the image",
            ));
        }
        self.position = next as u64;
        Ok(self.position)
    }
}

impl EvidenceReader for RbdEvidenceReader {
    fn info(&self) -> &ReaderInfo {
        &self.info
    }

    fn preferred_read_granularity(&self) -> usize {
        RBD_READ_GRANULARITY_BYTES
    }
}

impl RbdReadError {
    fn into_io_error(self) -> io::Error {
        let kind = match self {
            Self::UnsupportedFeatures { .. }
            | Self::JournalingUnsupported
            | Self::ParentCloneUnsupported
            | Self::SnapshotUnsupported
            | Self::EncryptionUnsupported
            | Self::OperationFeaturesUnsupported { .. } => io::ErrorKind::Unsupported,
            Self::RangeOutOfBounds { .. } | Self::RangeOverflow => io::ErrorKind::InvalidInput,
            Self::Provider(_) => io::ErrorKind::Other,
            Self::InvalidLayout(_)
            | Self::ObjectIdentityMismatch { .. }
            | Self::ShortObjectRead { .. } => io::ErrorKind::InvalidData,
        };
        io::Error::new(kind, self)
    }
}

fn validate_features(features: u64) -> Result<(), RbdReadError> {
    if features & RBD_FEATURE_JOURNALING != 0 {
        return Err(RbdReadError::JournalingUnsupported);
    }
    let unsupported = features
        & (RBD_FEATURE_DATA_POOL
            | RBD_FEATURE_MIGRATING
            | RBD_FEATURE_DIRTY_CACHE
            | !RBD_FEATURES_ALL);
    if unsupported == 0 {
        Ok(())
    } else {
        Err(RbdReadError::UnsupportedFeatures {
            features,
            unsupported,
        })
    }
}

fn validate_context(context: &RbdReadContext) -> Result<(), RbdReadError> {
    if context.has_parent
        || context.operation_features
            & (RBD_OPERATION_FEATURE_CLONE_PARENT | RBD_OPERATION_FEATURE_CLONE_CHILD)
            != 0
    {
        return Err(RbdReadError::ParentCloneUnsupported);
    }
    if context.operation_features != 0 {
        return Err(RbdReadError::OperationFeaturesUnsupported {
            features: context.operation_features,
        });
    }
    if context.snapshot_id.is_some() {
        return Err(RbdReadError::SnapshotUnsupported);
    }
    if context.encrypted {
        return Err(RbdReadError::EncryptionUnsupported);
    }
    Ok(())
}

#[cfg(test)]
#[path = "../../tests/unit/ceph_reconstruction/rbd_reader.rs"]
mod tests;
