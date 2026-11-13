use super::{SourceStorageHandle, StorageRuntimeError};
use persistence_fjall::{
    ArtifactRecordStore, CatalogRecordStore, ScanDirection, SourceReadSnapshot,
};

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

    pub fn artifact_type_counts(
        &self,
        snapshot: &SourceReadSnapshot,
        source: &str,
    ) -> Result<Vec<(String, u64)>, StorageRuntimeError> {
        let artifacts = ArtifactRecordStore::from_source_store(self.source_store().clone());
        if !artifacts.type_counts_ready(snapshot, source)? {
            let first =
                artifacts.list_by_source(snapshot, source, None, ScanDirection::Forward, 1)?;
            if first.records.is_empty() {
                return Ok(Vec::new());
            }
            return Err(StorageRuntimeError::SourceNotReady(source.to_owned()));
        }
        artifacts.type_counts(snapshot, source).map_err(Into::into)
    }
}
