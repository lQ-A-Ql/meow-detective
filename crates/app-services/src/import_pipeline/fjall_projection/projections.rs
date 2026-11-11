use crate::storage_runtime::ReadWriteSourceStorageHandle;
use persistence_fjall::{
    ArtifactRecordStore, CatalogRecordStore, PersistMode, ScanDirection, SourceStore,
    TimelineRecordStore,
};
use rusqlite::Connection;
use std::sync::Arc;
use transport::CommandError;

const PAGE_SIZE: usize = 512;

fn origin(connection: &Connection, source_id: &str) -> Result<Arc<SourceStore>, CommandError> {
    let owner = persistence_source::detail::source_id(connection)
        .map_err(|error| CommandError::internal(error.to_string()))?;
    if owner.as_deref() != Some(source_id) {
        return Err(CommandError::internal(
            "source generation owner does not match projection source",
        ));
    }
    persistence_source::detail::open_store(connection)
        .map_err(|error| CommandError::internal(error.to_string()))?
        .ok_or_else(|| CommandError::internal("source generation is missing"))
}

pub(super) fn mirror_artifacts(
    case_connection: &Connection,
    case_root: &std::path::Path,
    case_id: &str,
    connection: &Connection,
    source: &ReadWriteSourceStorageHandle,
    source_id: &str,
) -> Result<(), CommandError> {
    mirror_artifact_records_only(connection, source, source_id)?;
    let objects = artifact_storage::ArtifactStore::new(case_root)
        .map_err(|error| CommandError::internal(error.to_string()))?;
    let records = ArtifactRecordStore::from_source_store(source.source_store().clone());
    let snapshot = records.read_snapshot();
    let mut cursor = None;
    loop {
        let page = records
            .list_by_source(
                &snapshot,
                source_id,
                cursor.as_deref(),
                ScanDirection::Forward,
                PAGE_SIZE,
            )
            .map_err(|error| CommandError::internal(error.to_string()))?;
        let mut items = Vec::with_capacity(page.records.len());
        for record in page.records {
            let bytes = serde_json::to_vec(&record.attrs)
                .map_err(|error| CommandError::internal(error.to_string()))?;
            let stored = objects
                .put_bytes_buffered(&bytes)
                .map_err(|error| CommandError::internal(error.to_string()))?;
            items.push(crate::artifact_storage_service::ArtifactObjectBatchItem {
                stored,
                media_type: "application/json".into(),
                source_id: Some(source_id.into()),
                owner_kind: "artifact".into(),
                owner_id: record.id,
                role: "metadata".into(),
            });
        }
        objects
            .sync_objects(
                &items
                    .iter()
                    .map(|item| item.stored.clone())
                    .collect::<Vec<_>>(),
            )
            .map_err(|error| CommandError::internal(error.to_string()))?;
        crate::artifact_storage_service::register_artifact_objects_and_references_batch(
            case_connection,
            case_id,
            &items,
        )
        .map_err(|error| CommandError::internal(error.to_string()))?;
        cursor = page.next_cursor;
        if cursor.is_none() {
            break;
        }
    }
    Ok(())
}

pub(super) fn mirror_artifact_records_only(
    connection: &Connection,
    source: &ReadWriteSourceStorageHandle,
    source_id: &str,
) -> Result<(), CommandError> {
    let store = origin(connection, source_id)?;
    if store.database().path() == source.source_store().database().path() {
        return Ok(());
    }
    let records = ArtifactRecordStore::from_source_store((*store).clone());
    let snapshot = records.read_snapshot();
    let mut cursor = None;
    loop {
        let page = records
            .list_by_source(
                &snapshot,
                source_id,
                cursor.as_deref(),
                ScanDirection::Forward,
                PAGE_SIZE,
            )
            .map_err(|error| CommandError::internal(error.to_string()))?;
        source
            .artifact_upsert_batch(&page.records, PersistMode::Buffer)
            .map_err(|error| CommandError::internal(error.to_string()))?;
        cursor = page.next_cursor;
        if cursor.is_none() {
            break;
        }
    }
    Ok(())
}

pub(super) fn mirror_catalog(
    connection: &Connection,
    source: &ReadWriteSourceStorageHandle,
    source_id: &str,
) -> Result<(), CommandError> {
    let store = origin(connection, source_id)?;
    if store.database().path() == source.source_store().database().path() {
        return Ok(());
    }
    let records = CatalogRecordStore::from_source_store((*store).clone());
    let snapshot = records.read_snapshot();
    let mut cursor = None;
    loop {
        let page = records
            .list_all(
                &snapshot,
                source_id,
                cursor.as_deref(),
                ScanDirection::Forward,
                PAGE_SIZE,
            )
            .map_err(|error| CommandError::internal(error.to_string()))?;
        source
            .catalog_upsert_batch(&page.records, PersistMode::Buffer)
            .map_err(|error| CommandError::internal(error.to_string()))?;
        cursor = page.next_cursor;
        if cursor.is_none() {
            break;
        }
    }
    Ok(())
}

pub(super) fn mirror_timeline(
    connection: &Connection,
    source: &ReadWriteSourceStorageHandle,
    source_id: &str,
) -> Result<(), CommandError> {
    let store = origin(connection, source_id)?;
    if store.database().path() == source.source_store().database().path() {
        return Ok(());
    }
    let records = TimelineRecordStore::from_source_store((*store).clone());
    let snapshot = records.read_snapshot();
    let mut cursor = None;
    loop {
        let page = records
            .list_by_source(
                &snapshot,
                source_id,
                cursor.as_deref(),
                ScanDirection::Forward,
                PAGE_SIZE,
            )
            .map_err(|error| CommandError::internal(error.to_string()))?;
        source
            .timeline_upsert_batch(&page.records, PersistMode::Buffer)
            .map_err(|error| CommandError::internal(error.to_string()))?;
        cursor = page.next_cursor;
        if cursor.is_none() {
            break;
        }
    }
    Ok(())
}
