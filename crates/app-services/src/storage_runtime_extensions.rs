use crate::storage_runtime::{
    ReadOnlySourceStorageHandle, ReadWriteSourceStorageHandle, StorageRuntimeError,
};
use persistence_fjall::{
    ArtifactPage, ArtifactRecord, ArtifactRecordStore, ParserPage, ParserRecord, ParserRecordStore,
    PersistMode, ScanDirection, SourceReadSnapshot,
};

impl ReadOnlySourceStorageHandle {
    pub fn parser_list_by_family(
        &self,
        snapshot: &SourceReadSnapshot,
        source_id: &str,
        family: &str,
        cursor: Option<&[u8]>,
        direction: ScanDirection,
        limit: usize,
    ) -> Result<ParserPage, StorageRuntimeError> {
        ParserRecordStore::from_source_store(self.source_store().clone())
            .list_by_family(snapshot, source_id, family, cursor, direction, limit)
            .map_err(StorageRuntimeError::Source)
    }
    pub fn parser_list_by_kind(
        &self,
        snapshot: &SourceReadSnapshot,
        source_id: &str,
        kind: &str,
        cursor: Option<&[u8]>,
        direction: ScanDirection,
        limit: usize,
    ) -> Result<ParserPage, StorageRuntimeError> {
        ParserRecordStore::from_source_store(self.source_store().clone())
            .list_by_kind(snapshot, source_id, kind, cursor, direction, limit)
            .map_err(StorageRuntimeError::Source)
    }
    pub fn artifact_list_by_type(
        &self,
        snapshot: &SourceReadSnapshot,
        source_id: &str,
        artifact_type: &str,
        cursor: Option<&[u8]>,
        direction: ScanDirection,
        limit: usize,
    ) -> Result<ArtifactPage, StorageRuntimeError> {
        ArtifactRecordStore::from_source_store(self.source_store().clone())
            .list_by_type(snapshot, source_id, artifact_type, cursor, direction, limit)
            .map_err(StorageRuntimeError::Source)
    }
    pub fn artifact_list_by_source_object(
        &self,
        snapshot: &SourceReadSnapshot,
        source_id: &str,
        source_object_id: &str,
        cursor: Option<&[u8]>,
        direction: ScanDirection,
        limit: usize,
    ) -> Result<ArtifactPage, StorageRuntimeError> {
        ArtifactRecordStore::from_source_store(self.source_store().clone())
            .list_by_source_object(
                snapshot,
                source_id,
                source_object_id,
                cursor,
                direction,
                limit,
            )
            .map_err(StorageRuntimeError::Source)
    }
}

impl ReadWriteSourceStorageHandle {
    pub fn parser_upsert(
        &self,
        record: &ParserRecord,
        mode: PersistMode,
    ) -> Result<(), StorageRuntimeError> {
        ParserRecordStore::from_source_store(self.source_store().clone())
            .upsert(record, mode)
            .map_err(StorageRuntimeError::Source)
    }
    pub fn artifact_upsert(
        &self,
        record: &ArtifactRecord,
        mode: PersistMode,
    ) -> Result<(), StorageRuntimeError> {
        ArtifactRecordStore::from_source_store(self.source_store().clone())
            .upsert(record, mode)
            .map_err(StorageRuntimeError::Source)
    }
}
