//! Source-local catalog projection helpers shared by enumeration and import.
//!
//! The import pipeline and the file-service enumeration path both mirror the
//! catalog records of a source database into the active source store. The
//! helpers live here so the evidence layer never has to reach up into the
//! import pipeline.

use crate::storage_runtime::ReadWriteSourceStorageHandle;
use persistence_source::fjall::{
    encode_key, CatalogRecordStore, KeyPart, KeyspaceName, PersistMode, ScanDirection, SourceStore,
    VersionedEnvelope,
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

/// Mirrors the source catalog into the active source store of `source`.
pub fn mirror_catalog_records_from_connection(
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

/// Marks the catalog projection of `source` as seeded for `source_id`.
pub fn mark_catalog_projection_seeded(
    source: &ReadWriteSourceStorageHandle,
    source_id: &str,
) -> Result<(), CommandError> {
    let key = encode_key("projection", &[KeyPart::Text(source_id.to_owned())])
        .map_err(|error| CommandError::internal(error.to_string()))?;
    let marker = VersionedEnvelope::new(
        "catalog_projection_seeded",
        1,
        format!("projection:{source_id}"),
        source_id,
        1,
        source_id.as_bytes().to_vec(),
    );
    source
        .write_envelope(KeyspaceName::Meta, key, &marker, PersistMode::SyncAll)
        .map_err(|error| CommandError::internal(error.to_string()))?;
    Ok(())
}
