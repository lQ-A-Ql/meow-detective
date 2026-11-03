use super::{
    copy_and_hash, sync_directory, unique_suffix, ArtifactStorageError, ArtifactStore,
    ObjectAddress, StoredObject,
};
use std::fs::{self, OpenOptions};
use std::io::{self, Read};

impl ArtifactStore {
    pub fn put_bytes_buffered(&self, bytes: &[u8]) -> Result<StoredObject, ArtifactStorageError> {
        self.put_reader_buffered(io::Cursor::new(bytes))
    }

    pub fn sync_objects(&self, objects: &[StoredObject]) -> Result<(), ArtifactStorageError> {
        let mut directories = std::collections::BTreeSet::new();
        for object in objects {
            if let Some(parent) = object.path.parent() {
                directories.insert(parent.to_path_buf());
            }
        }
        for directory in directories {
            sync_directory(Some(&directory))?;
        }
        Ok(())
    }

    fn put_reader_buffered<R: Read>(
        &self,
        mut reader: R,
    ) -> Result<StoredObject, ArtifactStorageError> {
        let staging_dir = self.objects_root.join(".staging");
        fs::create_dir_all(&staging_dir)?;
        let temporary_path =
            staging_dir.join(format!("{}.{}.tmp", std::process::id(), unique_suffix()));
        let mut temporary = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary_path)?;
        let (digest, length) = copy_and_hash(&mut reader, &mut temporary)?;
        drop(temporary);
        let address = ObjectAddress::from_digest(digest);
        let destination = self.object_path(&address);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        let result = if destination.exists() {
            self.reuse_existing(&destination, &address, length)
        } else {
            match fs::rename(&temporary_path, &destination) {
                Ok(()) => Ok(StoredObject {
                    address: address.clone(),
                    length,
                    path: destination.clone(),
                    reused: false,
                }),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                    self.reuse_existing(&destination, &address, length)
                }
                Err(error) => Err(ArtifactStorageError::Io(error)),
            }
        };
        if result.as_ref().map_or(true, |stored| stored.reused) {
            let _ = fs::remove_file(&temporary_path);
        }
        result
    }
}
