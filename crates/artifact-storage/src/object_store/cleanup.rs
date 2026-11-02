use super::{ArtifactStorageError, ArtifactStore, ObjectAddress};
use std::fs;
use std::io;

impl ArtifactStore {
    pub fn remove(&self, address: &ObjectAddress) -> Result<bool, ArtifactStorageError> {
        let path = self.object_path(address);
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(ArtifactStorageError::Io(error)),
        };
        if metadata.file_type().is_symlink() {
            return Err(ArtifactStorageError::SymlinkObject(path));
        }
        fs::remove_file(path)?;
        Ok(true)
    }
}
