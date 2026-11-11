use super::{SourceStorageHandle, StorageRuntimeError};
use persistence_fjall::{ArtifactRecordStore, CatalogRecordStore, SourceReadSnapshot};

impl SourceStorageHandle<super::ReadOnly> {
    pub fn require_projection_complete(
        &self,
        snapshot: &SourceReadSnapshot,
        source: &str,
    ) -> Result<(), StorageRuntimeError> {
        if !self.source_projection_complete(snapshot, source)? {
            return Err(StorageRuntimeError::SourceNotReady(source.into()));
        }
        Ok(())
    }

    pub fn catalog_count(
        &self,
        snapshot: &SourceReadSnapshot,
        source: &str,
    ) -> Result<u64, StorageRuntimeError> {
        CatalogRecordStore::from_source_store(self.source_store().clone())
            .count(snapshot, Some(source))
            .map_err(Into::into)
    }

    pub fn artifact_count(
        &self,
        snapshot: &SourceReadSnapshot,
        source: &str,
    ) -> Result<u64, StorageRuntimeError> {
        ArtifactRecordStore::from_source_store(self.source_store().clone())
            .count(snapshot, Some(source))
            .map_err(Into::into)
    }
}
