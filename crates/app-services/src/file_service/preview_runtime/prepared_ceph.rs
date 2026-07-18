use std::sync::{Arc, Mutex};

use evidence_core::FileSystemReader;

use crate::file_service::{
    viewer::{descriptor_image_path_candidates, PreviewDescriptor},
    FileServiceError,
};

pub(super) type SharedPreparedFilesystem = Arc<Mutex<Box<dyn FileSystemReader + Send>>>;

pub(super) struct PreparedCephFile {
    filesystem: SharedPreparedFilesystem,
    path_candidates: Vec<String>,
    resolved_path: Option<String>,
    size: u64,
}

impl PreparedCephFile {
    pub(super) fn open(
        filesystem: SharedPreparedFilesystem,
        descriptor: &PreviewDescriptor,
    ) -> Result<Self, FileServiceError> {
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
        let filesystem = self
            .filesystem
            .lock()
            .map_err(|_| FileServiceError::other("Shared preview filesystem lock is poisoned"))?;
        if let Some(path) = self.resolved_path.as_deref() {
            return filesystem
                .read_file_range(path, offset, bounded_length)
                .map_err(FileServiceError::Io);
        }

        let mut failures = Vec::new();
        for path in &self.path_candidates {
            match filesystem.read_file_range(path, offset, bounded_length) {
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
