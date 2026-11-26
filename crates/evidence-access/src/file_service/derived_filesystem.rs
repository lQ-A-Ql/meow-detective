use evidence_core::{EvidenceReader, FileSystemReader};

use ::ceph_reconstruction::ceph_reconstruction::DerivedRbdRuntime;

use crate::file_service::{viewer::PreviewPartitionCandidate, FileServiceError};

pub(super) fn open_derived_filesystem(
    runtime: &DerivedRbdRuntime,
    candidate: &PreviewPartitionCandidate,
) -> Result<Box<dyn FileSystemReader + Send>, FileServiceError> {
    let (reader, filesystem_offset) = if let Some(identity) = &candidate.lvm_identity {
        let base = runtime
            .open_reader()
            .map_err(|error| FileServiceError::other(error.to_string()))?;
        let pool = fs_lvm::LvmPool::discover(
            vec![Box::new(base) as Box<dyn EvidenceReader>],
            identity.pv_offsets.clone(),
        )
        .map_err(|error| FileServiceError::other(error.to_string()))?;
        let volume_index = pool
            .list_volumes()
            .iter()
            .position(|volume| {
                (!identity.lv_uuid.is_empty() && volume.uuid == identity.lv_uuid)
                    || volume.name == identity.lv_name
            })
            .ok_or_else(|| {
                FileServiceError::other(format!(
                    "Ceph RBD LVM logical volume '{}' was not found",
                    identity.lv_name
                ))
            })?;
        (
            pool.open_volume_reader(volume_index)
                .map_err(|error| FileServiceError::other(error.to_string()))?,
            0,
        )
    } else {
        (
            Box::new(
                runtime
                    .open_reader()
                    .map_err(|error| FileServiceError::other(error.to_string()))?,
            ) as Box<dyn EvidenceReader>,
            candidate.offset,
        )
    };

    super::open_block_filesystem(&candidate.filesystem_kind, reader, filesystem_offset)
}
