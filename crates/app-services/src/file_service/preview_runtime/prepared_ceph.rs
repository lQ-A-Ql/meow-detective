use std::sync::Arc;

use evidence_core::{EvidenceReader, FileSystemReader};

use crate::{
    ceph_reconstruction::DerivedRbdRuntime,
    file_service::{
        viewer::{descriptor_image_path_candidates, PreviewDescriptor, PreviewPartitionCandidate},
        FileServiceError,
    },
};

pub(super) struct PreparedCephFile {
    filesystem: Box<dyn FileSystemReader + Send>,
    path_candidates: Vec<String>,
    resolved_path: Option<String>,
    size: u64,
}

impl PreparedCephFile {
    pub(super) fn open(
        runtime: Arc<DerivedRbdRuntime>,
        descriptor: &PreviewDescriptor,
    ) -> Result<Self, FileServiceError> {
        let candidate = descriptor.partition_candidates.first().ok_or_else(|| {
            FileServiceError::other("Ceph RBD preview has no partition candidate")
        })?;
        let filesystem = open_filesystem(&runtime, candidate)?;
        let path_candidates = descriptor_image_path_candidates(descriptor);
        if path_candidates.is_empty() {
            return Err(FileServiceError::other(
                "Ceph RBD preview has no usable file path",
            ));
        }
        Ok(Self {
            filesystem,
            path_candidates,
            resolved_path: None,
            size: descriptor.size,
        })
    }

    pub(super) fn read_range(
        &mut self,
        offset: u64,
        length: usize,
    ) -> Result<Vec<u8>, FileServiceError> {
        if offset > self.size {
            return Err(FileServiceError::other("Read offset exceeds file size"));
        }
        let bounded_length = length.min(self.size.saturating_sub(offset) as usize);
        if let Some(path) = self.resolved_path.as_deref() {
            return self
                .filesystem
                .read_file_range(path, offset, bounded_length)
                .map_err(FileServiceError::Io);
        }

        let mut failures = Vec::new();
        for path in &self.path_candidates {
            match self
                .filesystem
                .read_file_range(path, offset, bounded_length)
            {
                Ok(bytes) => {
                    self.resolved_path = Some(path.clone());
                    return Ok(bytes);
                }
                Err(error) => failures.push(format!("'{path}': {error}")),
            }
        }
        Err(FileServiceError::other(format!(
            "Ceph RBD file path could not be resolved: {}",
            failures.join("; ")
        )))
    }
}

fn open_filesystem(
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
