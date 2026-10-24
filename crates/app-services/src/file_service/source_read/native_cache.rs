//! Reuse immutable native filesystems within one source-bound read operation.
use super::SourceReadContext;
use crate::file_service::{
    viewer::{
        descriptor_image_path_candidates, exact_partition_candidate, is_linux_filesystem_kind,
        open_filesystem_reader, PreviewDescriptor, PreviewReadContext,
    },
    FileServiceError,
};

impl SourceReadContext<'_> {
    pub(super) fn read_native_header(
        &mut self,
        descriptor: &PreviewDescriptor,
        maximum: usize,
    ) -> Result<Option<Vec<u8>>, FileServiceError> {
        if !matches!(descriptor.source_kind.as_str(), "e01" | "raw") {
            return Ok(None);
        }
        if descriptor.data_source_id != self.data_source_id.0 {
            return Err(FileServiceError::security(
                "Native read descriptor belongs to another source",
            ));
        }
        let candidate = exact_partition_candidate(descriptor)?;
        if !is_linux_filesystem_kind(&candidate.filesystem_kind) {
            return Ok(None);
        }
        let key = serde_json::to_string(&(descriptor.source_path.as_str(), candidate))
            .map_err(|error| FileServiceError::other(error.to_string()))?;
        if !self.native_filesystems.contains_key(&key) {
            if self.native_filesystems.len() >= 4 {
                self.native_filesystems.clear();
            }
            let (reader, offset, _) = self.open_candidate_block_reader(descriptor, candidate)?;
            let filesystem = open_filesystem_reader(candidate, reader, offset)?;
            self.native_filesystems.insert(key.clone(), filesystem);
        }
        let filesystem = self
            .native_filesystems
            .get(&key)
            .ok_or_else(|| FileServiceError::other("Native filesystem was not initialized"))?;
        let length = maximum.min(usize::try_from(descriptor.size).unwrap_or(usize::MAX));
        let mut attempts = Vec::new();
        for path in descriptor_image_path_candidates(descriptor) {
            match filesystem.read_file_range(&path, 0, length) {
                Ok(bytes) => return Ok(Some(bytes)),
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::NotFound
                            | std::io::ErrorKind::NotADirectory
                            | std::io::ErrorKind::IsADirectory
                    ) =>
                {
                    attempts.push(error.to_string())
                }
                Err(error) => return Err(FileServiceError::Io(error)),
            }
        }
        Err(FileServiceError::other(format!(
            "Native file path could not be resolved: {}",
            attempts.join("; ")
        )))
    }
}
