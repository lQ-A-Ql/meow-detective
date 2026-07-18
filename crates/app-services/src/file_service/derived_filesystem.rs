use evidence_core::{EvidenceReader, FileSystemReader};

use crate::{
    ceph_reconstruction::DerivedRbdRuntime,
    file_service::{viewer::PreviewPartitionCandidate, FileServiceError},
};

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

    match candidate.filesystem_kind.to_ascii_lowercase().as_str() {
        "ext4" => fs_ext4::Ext4Reader::open(reader, filesystem_offset)
            .map(|filesystem| Box::new(filesystem) as Box<dyn FileSystemReader + Send>)
            .map_err(FileServiceError::Io),
        "xfs" => fs_xfs::XfsReader::open(reader, filesystem_offset)
            .map(|filesystem| Box::new(filesystem) as Box<dyn FileSystemReader + Send>)
            .map_err(FileServiceError::Io),
        "btrfs" => fs_btrfs::BtrfsReader::open(reader, filesystem_offset)
            .map(|filesystem| Box::new(filesystem) as Box<dyn FileSystemReader + Send>)
            .map_err(FileServiceError::Io),
        other => Err(FileServiceError::Unsupported(format!(
            "Ceph RBD preview does not support filesystem '{other}'",
        ))),
    }
}
