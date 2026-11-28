use evidence_core::FileSystemReader;

use ::ceph_reconstruction::ceph_reconstruction::DerivedRbdRuntime;

use crate::file_service::{viewer::PreviewPartitionCandidate, FileServiceError};

pub(super) fn open_derived_filesystem(
    runtime: &DerivedRbdRuntime,
    candidate: &PreviewPartitionCandidate,
) -> Result<Box<dyn FileSystemReader + Send>, FileServiceError> {
    let reader = runtime
        .open_reader()
        .map_err(|error| FileServiceError::other(error.to_string()))?;
    super::native_filesystem::open_preview_filesystem(reader, candidate)
}
