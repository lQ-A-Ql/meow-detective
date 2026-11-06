use super::{ArtifactStorageError, ArtifactStore, ARTIFACTS_DIRECTORY, OBJECTS_DIRECTORY};
use std::{fs, path::Path};

impl ArtifactStore {
    pub fn new(case_root: impl AsRef<Path>) -> Result<Self, ArtifactStorageError> {
        let case_root = case_root.as_ref().to_path_buf();
        let objects_root = case_root.join(ARTIFACTS_DIRECTORY).join(OBJECTS_DIRECTORY);
        fs::create_dir_all(&objects_root)?;
        Ok(Self {
            case_root,
            objects_root,
        })
    }

    pub fn from_objects_root(objects_root: impl AsRef<Path>) -> Result<Self, ArtifactStorageError> {
        fs::create_dir_all(objects_root.as_ref())?;
        Self::from_existing_objects_root(objects_root)
    }

    pub fn from_existing_objects_root(
        objects_root: impl AsRef<Path>,
    ) -> Result<Self, ArtifactStorageError> {
        let objects_root = objects_root.as_ref().to_path_buf();
        let case_root = objects_root
            .parent()
            .and_then(Path::parent)
            .map_or_else(|| objects_root.clone(), Path::to_path_buf);
        Ok(Self {
            case_root,
            objects_root,
        })
    }
}
