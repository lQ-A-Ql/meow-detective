use crate::file_service::{viewer::descriptor_image_path_candidates, FileServiceError};

pub(crate) struct PreparedFilesystemFile {
    filesystem: Box<dyn evidence_core::FileSystemReader + Send>,
    path_candidates: Vec<String>,
    resolved_path: Option<String>,
    size: u64,
}

impl PreparedFilesystemFile {
    pub(crate) fn open(
        filesystem: Box<dyn evidence_core::FileSystemReader + Send>,
        descriptor: &crate::file_service::PreviewDescriptor,
    ) -> Result<Self, FileServiceError> {
        let path_candidates = descriptor_image_path_candidates(descriptor);
        if path_candidates.is_empty() {
            return Err(FileServiceError::other(
                "Prepared filesystem preview has no usable file path",
            ));
        }
        Ok(Self {
            filesystem,
            path_candidates,
            resolved_path: None,
            size: descriptor.size,
        })
    }

    pub(crate) fn read_range(
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
            "Prepared filesystem path could not be resolved: {}",
            failures.join("; ")
        )))
    }
}
