use crate::search_metadata_projection::SearchProjectionMarker;
use crate::storage_runtime::{
    ReadOnlySourceStorageHandle, ReadWriteSourceStorageHandle, StorageRuntimeError,
};
use persistence_fjall::{
    ArtifactPage, ArtifactRecord, ArtifactRecordStore, CatalogPage, CatalogRecordStore,
    FileEntryRecord, ParserFieldQuery, ParserPage, ParserRecord, ParserRecordStore, PersistMode,
    ScanDirection, SourceReadSnapshot,
};

impl ReadOnlySourceStorageHandle {
    pub fn catalog_list_directories_by_parent(
        &self,
        snapshot: &SourceReadSnapshot,
        source_id: &str,
        parent_id: Option<&str>,
        cursor: Option<&[u8]>,
        direction: ScanDirection,
        limit: usize,
    ) -> Result<CatalogPage, StorageRuntimeError> {
        CatalogRecordStore::from_source_store(self.source_store().clone())
            .list_directories_by_parent(snapshot, source_id, parent_id, cursor, direction, limit)
            .map_err(StorageRuntimeError::Source)
    }
    pub fn catalog_list_orphan_directories(
        &self,
        snapshot: &SourceReadSnapshot,
        source_id: &str,
        cursor: Option<&[u8]>,
        direction: ScanDirection,
        limit: usize,
    ) -> Result<CatalogPage, StorageRuntimeError> {
        CatalogRecordStore::from_source_store(self.source_store().clone())
            .list_orphan_directories(snapshot, source_id, cursor, direction, limit)
            .map_err(StorageRuntimeError::Source)
    }
    pub fn parser_get(
        &self,
        snapshot: &SourceReadSnapshot,
        source_id: &str,
        record_id: &str,
    ) -> Result<Option<ParserRecord>, StorageRuntimeError> {
        ParserRecordStore::from_source_store(self.source_store().clone())
            .get(snapshot, source_id, record_id)
            .map_err(StorageRuntimeError::Source)
    }
    pub(crate) fn search_projection_marker(
        &self,
        snapshot: &SourceReadSnapshot,
        source_id: &str,
    ) -> Result<Option<SearchProjectionMarker>, StorageRuntimeError> {
        let key = persistence_fjall::encode_key(
            "search",
            &[persistence_fjall::KeyPart::Text(source_id.to_owned())],
        )
        .map_err(StorageRuntimeError::Source)?;
        let Some(envelope) =
            self.read_envelope(snapshot, persistence_fjall::KeyspaceName::Meta, &key)?
        else {
            return Ok(None);
        };
        if envelope.record_kind != "search_projection_generation" {
            return Ok(None);
        }
        envelope
            .decode_payload()
            .map(Some)
            .map_err(StorageRuntimeError::Source)
    }

    pub fn search_projection_present(
        &self,
        snapshot: &SourceReadSnapshot,
        source_id: &str,
    ) -> Result<bool, StorageRuntimeError> {
        let key = persistence_fjall::encode_key(
            "search",
            &[persistence_fjall::KeyPart::Text(source_id.to_owned())],
        )
        .map_err(StorageRuntimeError::Source)?;
        Ok(self
            .read_envelope(snapshot, persistence_fjall::KeyspaceName::Meta, &key)?
            .is_some_and(|envelope| envelope.record_kind == "search_projection_generation"))
    }
    pub fn artifact_list_by_source(
        &self,
        snapshot: &SourceReadSnapshot,
        source_id: &str,
        cursor: Option<&[u8]>,
        direction: ScanDirection,
        limit: usize,
    ) -> Result<ArtifactPage, StorageRuntimeError> {
        ArtifactRecordStore::from_source_store(self.source_store().clone())
            .list_by_source(snapshot, source_id, cursor, direction, limit)
            .map_err(StorageRuntimeError::Source)
    }
    pub fn artifact_get(
        &self,
        snapshot: &SourceReadSnapshot,
        source_id: &str,
        artifact_id: &str,
    ) -> Result<Option<ArtifactRecord>, StorageRuntimeError> {
        ArtifactRecordStore::from_source_store(self.source_store().clone())
            .get(snapshot, source_id, artifact_id)
            .map_err(StorageRuntimeError::Source)
    }
    pub fn batch_receipt_state(
        &self,
        digest: &[u8; 32],
    ) -> Result<persistence_fjall::BatchReceiptState, StorageRuntimeError> {
        self.source_store()
            .database()
            .batch_receipt_state_for_digest(digest)
            .map_err(StorageRuntimeError::Source)
    }
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
    pub fn parser_list_by_field(
        &self,
        snapshot: &SourceReadSnapshot,
        query: ParserFieldQuery<'_>,
        cursor: Option<&[u8]>,
        direction: ScanDirection,
        limit: usize,
    ) -> Result<ParserPage, StorageRuntimeError> {
        ParserRecordStore::from_source_store(self.source_store().clone())
            .list_by_field(snapshot, query, cursor, direction, limit)
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
    pub fn catalog_upsert_batch(
        &self,
        records: &[FileEntryRecord],
        mode: PersistMode,
    ) -> Result<(), StorageRuntimeError> {
        CatalogRecordStore::from_source_store(self.source_store().clone())
            .upsert_batch(records, mode)
            .map_err(StorageRuntimeError::Source)
    }
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
