use ceph_wire::{CephWireError, RbdHeadImageLayout};
use thiserror::Error;

use crate::datasource_service::{self, DataSourceError, ImageFilesystemProbe};

use super::{RbdEvidenceReader, RbdImageDescriptor, RbdObjectProvider, RbdReadError};

#[derive(Debug, Error)]
pub enum RbdImageError {
    #[error("RBD image {image_id} has an invalid head-image layout")]
    InvalidLayout {
        image_id: String,
        #[source]
        source: CephWireError,
    },
    #[error("RBD image {image_id} cannot be opened as a bounded head image")]
    Open {
        image_id: String,
        #[source]
        source: RbdReadError,
    },
    #[error("RBD image {image_id} filesystem detection failed")]
    FilesystemDetection {
        image_id: String,
        #[source]
        source: DataSourceError,
    },
}

pub fn build_rbd_head_layout(
    descriptor: &RbdImageDescriptor,
) -> Result<RbdHeadImageLayout, RbdImageError> {
    RbdHeadImageLayout::from_metadata(&descriptor.metadata).map_err(|source| {
        RbdImageError::InvalidLayout {
            image_id: descriptor.metadata.id.clone(),
            source,
        }
    })
}

pub fn open_rbd_head_image(
    descriptor: &RbdImageDescriptor,
    provider: Box<dyn RbdObjectProvider>,
) -> Result<RbdEvidenceReader, RbdImageError> {
    let layout = build_rbd_head_layout(descriptor)?;
    RbdEvidenceReader::new(provider, layout, descriptor.context.clone()).map_err(|source| {
        RbdImageError::Open {
            image_id: descriptor.metadata.id.clone(),
            source,
        }
    })
}

pub fn detect_rbd_image_filesystem(
    descriptor: &RbdImageDescriptor,
    provider: Box<dyn RbdObjectProvider>,
) -> Result<ImageFilesystemProbe, RbdImageError> {
    let mut reader = open_rbd_head_image(descriptor, provider)?;
    datasource_service::detect_image_filesystem(&mut reader).map_err(|source| {
        RbdImageError::FilesystemDetection {
            image_id: descriptor.metadata.id.clone(),
            source,
        }
    })
}

#[cfg(test)]
#[path = "../../tests/unit/ceph_reconstruction/rbd_image.rs"]
mod tests;
