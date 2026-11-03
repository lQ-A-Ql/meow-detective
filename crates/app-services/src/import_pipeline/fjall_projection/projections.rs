use crate::storage_runtime::ReadWriteSourceStorageHandle;
use artifact_storage::ArtifactStore;
use chrono::Utc;
use persistence_fjall::{CatalogEntryType, FileEntryRecord, PersistMode, TimelineRecord};
use rusqlite::Connection;
use std::collections::{BTreeMap, HashSet};
use transport::CommandError;

const BATCH_SIZE: usize = 512;

pub(super) fn mirror_artifacts(
    case_connection: &Connection,
    case_root: &std::path::Path,
    case_id: &str,
    connection: &Connection,
    source: &ReadWriteSourceStorageHandle,
    source_id: &str,
) -> Result<(), CommandError> {
    let object_store =
        ArtifactStore::new(case_root).map_err(|error| CommandError::internal(error.to_string()))?;
    let mut statement = connection.prepare("SELECT id, artifact_type, source_object_id, summary, attrs, title, extractor_id, extractor_version, confidence, source_attribution, created_at FROM artifacts WHERE data_source_id = ?1 ORDER BY id ASC").map_err(|error| CommandError::internal(error.to_string()))?;
    let rows = statement
        .query_map([source_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                artifact_record_from_row(row, source_id)?,
            ))
        })
        .map_err(|error| CommandError::internal(error.to_string()))?;
    let mut batch = Vec::with_capacity(BATCH_SIZE);
    let mut object_batch = Vec::with_capacity(BATCH_SIZE);
    let mut current_ids = HashSet::new();
    for row in rows {
        let (artifact_id, record) =
            row.map_err(|error| CommandError::internal(error.to_string()))?;
        current_ids.insert(artifact_id.clone());
        batch.push(record.clone());
        let object_bytes = serde_json::to_vec(&record.attrs)
            .map_err(|error| CommandError::internal(error.to_string()))?;
        let stored = object_store
            .put_bytes_buffered(&object_bytes)
            .map_err(|error| CommandError::internal(error.to_string()))?;
        object_batch.push(crate::artifact_storage_service::ArtifactObjectBatchItem {
            stored,
            media_type: "application/json".to_owned(),
            source_id: Some(source_id.to_owned()),
            owner_kind: "artifact".to_owned(),
            owner_id: artifact_id,
            role: "metadata".to_owned(),
        });
        if batch.len() == BATCH_SIZE {
            source
                .artifact_upsert_batch(&batch, PersistMode::Buffer)
                .map_err(|error| CommandError::internal(error.to_string()))?;
            batch.clear();
        }
        if object_batch.len() == BATCH_SIZE {
            let stored_objects = object_batch
                .iter()
                .map(|item| item.stored.clone())
                .collect::<Vec<_>>();
            object_store
                .sync_objects(&stored_objects)
                .map_err(|error| CommandError::internal(error.to_string()))?;
            crate::artifact_storage_service::register_artifact_objects_and_references_batch(
                case_connection,
                case_id,
                &object_batch,
            )
            .map_err(|error| CommandError::internal(error.to_string()))?;
            object_batch.clear();
        }
    }
    if !batch.is_empty() {
        source
            .artifact_upsert_batch(&batch, PersistMode::Buffer)
            .map_err(|error| CommandError::internal(error.to_string()))?;
    }
    if !object_batch.is_empty() {
        let stored_objects = object_batch
            .iter()
            .map(|item| item.stored.clone())
            .collect::<Vec<_>>();
        object_store
            .sync_objects(&stored_objects)
            .map_err(|error| CommandError::internal(error.to_string()))?;
        crate::artifact_storage_service::register_artifact_objects_and_references_batch(
            case_connection,
            case_id,
            &object_batch,
        )
        .map_err(|error| CommandError::internal(error.to_string()))?;
    }
    delete_stale_artifacts(case_connection, case_id, source, source_id, &current_ids)?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn mirror_artifact_ids(
    case_connection: &Connection,
    case_root: &std::path::Path,
    case_id: &str,
    connection: &Connection,
    source: &ReadWriteSourceStorageHandle,
    source_id: &str,
    artifact_ids: &[String],
) -> Result<(), CommandError> {
    if artifact_ids.is_empty() {
        return Ok(());
    }
    let placeholders = (2..=artifact_ids.len() + 1)
        .map(|index| format!("?{index}"))
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "SELECT id, artifact_type, source_object_id, summary, attrs, title,
                extractor_id, extractor_version, confidence, source_attribution, created_at
         FROM artifacts WHERE data_source_id = ?1 AND id IN ({placeholders})"
    );
    let mut parameters = Vec::with_capacity(artifact_ids.len() + 1);
    parameters.push(rusqlite::types::Value::Text(source_id.to_owned()));
    parameters.extend(
        artifact_ids
            .iter()
            .cloned()
            .map(rusqlite::types::Value::Text),
    );
    let mut statement = connection
        .prepare(&sql)
        .map_err(|error| CommandError::internal(error.to_string()))?;
    let rows = statement
        .query_map(rusqlite::params_from_iter(parameters), |row| {
            artifact_record_from_row(row, source_id)
        })
        .map_err(|error| CommandError::internal(error.to_string()))?;
    let records = rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| CommandError::internal(error.to_string()))?;
    let existing_ids = records
        .iter()
        .map(|record| record.id.clone())
        .collect::<HashSet<_>>();
    let object_store =
        ArtifactStore::new(case_root).map_err(|error| CommandError::internal(error.to_string()))?;
    for batch in records.chunks(BATCH_SIZE) {
        persist_artifact_records(
            case_connection,
            case_id,
            source_id,
            source,
            &object_store,
            batch,
        )?;
    }
    let deleted = artifact_ids
        .iter()
        .filter(|id| !existing_ids.contains(*id))
        .cloned()
        .collect::<Vec<_>>();
    crate::artifact_storage_service::release_artifact_owner_references_batch(
        case_connection,
        case_id,
        source_id,
        "artifact",
        "metadata",
        &deleted,
    )
    .map_err(|error| CommandError::internal(error.to_string()))?;
    source
        .artifact_delete_batch(source_id, &deleted, PersistMode::Buffer)
        .map_err(|error| CommandError::internal(error.to_string()))?;
    Ok(())
}

fn artifact_record_from_row(
    row: &rusqlite::Row<'_>,
    source_id: &str,
) -> rusqlite::Result<persistence_fjall::ArtifactRecord> {
    let summary: String = row.get(3)?;
    let attrs: String = row.get(4)?;
    let payload = serde_json::json!({
        "attrs": serde_json::from_str::<serde_json::Value>(&attrs).unwrap_or(serde_json::Value::Null),
        "title": row.get::<_, String>(5)?,
        "summary": summary,
        "extractorId": row.get::<_, Option<String>>(6)?,
        "extractorVersion": row.get::<_, Option<String>>(7)?,
        "confidence": row.get::<_, Option<f32>>(8)?,
        "sourceAttribution": row.get::<_, Option<String>>(9)?,
        "createdAt": row.get::<_, String>(10)?,
    });
    Ok(persistence_fjall::ArtifactRecord {
        id: row.get(0)?,
        data_source_id: source_id.to_owned(),
        artifact_type: row.get(1)?,
        source_object_id: row.get(2)?,
        content_sha256: Some(infrastructure::hashing::sha256_bytes(summary.as_bytes())),
        attrs: payload,
    })
}

fn persist_artifact_records(
    case_connection: &Connection,
    case_id: &str,
    source_id: &str,
    source: &ReadWriteSourceStorageHandle,
    object_store: &ArtifactStore,
    records: &[persistence_fjall::ArtifactRecord],
) -> Result<(), CommandError> {
    if records.is_empty() {
        return Ok(());
    }
    source
        .artifact_upsert_batch(records, PersistMode::Buffer)
        .map_err(|error| CommandError::internal(error.to_string()))?;
    let mut items = Vec::with_capacity(records.len());
    for record in records {
        let object_bytes = serde_json::to_vec(&record.attrs)
            .map_err(|error| CommandError::internal(error.to_string()))?;
        let stored = object_store
            .put_bytes_buffered(&object_bytes)
            .map_err(|error| CommandError::internal(error.to_string()))?;
        items.push(crate::artifact_storage_service::ArtifactObjectBatchItem {
            stored,
            media_type: "application/json".to_owned(),
            source_id: Some(source_id.to_owned()),
            owner_kind: "artifact".to_owned(),
            owner_id: record.id.clone(),
            role: "metadata".to_owned(),
        });
    }
    let stored = items
        .iter()
        .map(|item| item.stored.clone())
        .collect::<Vec<_>>();
    object_store
        .sync_objects(&stored)
        .map_err(|error| CommandError::internal(error.to_string()))?;
    crate::artifact_storage_service::register_artifact_objects_and_references_batch(
        case_connection,
        case_id,
        &items,
    )
    .map_err(|error| CommandError::internal(error.to_string()))?;
    Ok(())
}

fn delete_stale_artifacts(
    case_connection: &Connection,
    case_id: &str,
    source: &ReadWriteSourceStorageHandle,
    source_id: &str,
    current_ids: &HashSet<String>,
) -> Result<(), CommandError> {
    let snapshot = source.snapshot();
    let mut cursor = None;
    loop {
        let page = source
            .artifact_list_by_source(
                &snapshot,
                source_id,
                cursor.as_deref(),
                persistence_fjall::ScanDirection::Forward,
                BATCH_SIZE,
            )
            .map_err(|error| CommandError::internal(error.to_string()))?;
        let stale = page
            .records
            .into_iter()
            .map(|record| record.id)
            .filter(|id| !current_ids.contains(id))
            .collect::<Vec<_>>();
        crate::artifact_storage_service::release_artifact_owner_references_batch(
            case_connection,
            case_id,
            source_id,
            "artifact",
            "metadata",
            &stale,
        )
        .map_err(|error| CommandError::internal(error.to_string()))?;
        source
            .artifact_delete_batch(source_id, &stale, PersistMode::Buffer)
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
    let mut statement = connection.prepare("SELECT id, parent_id, data_source_id, path, name, entry_type, size, ext, deleted, hidden, system, created_at, modified_at, accessed_at, changed_at, hash_sha256, encrypted, read_only, archive, unix_mode, partition_index FROM file_entries WHERE data_source_id = ?1 ORDER BY id ASC").map_err(|error| CommandError::internal(error.to_string()))?;
    let rows = statement
        .query_map([source_id], |row| {
            let entry_type: String = row.get(5)?;
            Ok(FileEntryRecord {
                id: row.get(0)?,
                parent_id: row.get(1)?,
                data_source_id: row.get(2)?,
                path: row.get(3)?,
                name: row.get(4)?,
                entry_type: if entry_type.eq_ignore_ascii_case("directory") {
                    CatalogEntryType::Directory
                } else {
                    CatalogEntryType::File
                },
                size: row.get(6)?,
                ext: row.get(7)?,
                deleted: row.get::<_, i64>(8)? != 0,
                hidden: row.get::<_, i64>(9)? != 0,
                system: row.get::<_, i64>(10)? != 0,
                read_only: row.get::<_, i64>(17)? != 0,
                archive: row.get::<_, i64>(18)? != 0,
                encrypted: row.get::<_, i64>(16)? != 0,
                created: parse_optional_time(row.get(11)?),
                modified: parse_optional_time(row.get(12)?),
                accessed: parse_optional_time(row.get(13)?),
                changed: parse_optional_time(row.get(14)?),
                hash_sha256: row.get(15)?,
                partition_index: row.get(20)?,
                unix_mode: row.get(19)?,
            })
        })
        .map_err(|error| CommandError::internal(error.to_string()))?;
    let mut batch = Vec::with_capacity(BATCH_SIZE);
    for row in rows {
        batch.push(row.map_err(|error| CommandError::internal(error.to_string()))?);
        if batch.len() == BATCH_SIZE {
            source
                .catalog_upsert_batch(&batch, PersistMode::Buffer)
                .map_err(|error| CommandError::internal(error.to_string()))?;
            batch.clear();
        }
    }
    if !batch.is_empty() {
        source
            .catalog_upsert_batch(&batch, PersistMode::Buffer)
            .map_err(|error| CommandError::internal(error.to_string()))?;
    }
    Ok(())
}

pub(super) fn mirror_timeline(
    connection: &Connection,
    source: &ReadWriteSourceStorageHandle,
    source_id: &str,
) -> Result<(), CommandError> {
    let mut statement = connection.prepare("SELECT id, source_object_id, event_type, ts, title, description, parser_id, parser_version, confidence, source_attribution, attrs FROM timeline_events ORDER BY ts DESC, id ASC").map_err(|error| CommandError::internal(error.to_string()))?;
    let rows = statement
        .query_map([], |row| timeline_record_from_row(row, source_id))
        .map_err(|error| CommandError::internal(error.to_string()))?;
    let mut batch = Vec::with_capacity(BATCH_SIZE);
    let mut current_ids = HashSet::new();
    for row in rows {
        let record = row.map_err(|error| CommandError::internal(error.to_string()))?;
        current_ids.insert(record.id.clone());
        batch.push(record);
        if batch.len() == BATCH_SIZE {
            source
                .timeline_upsert_batch(&batch, PersistMode::Buffer)
                .map_err(|error| CommandError::internal(error.to_string()))?;
            batch.clear();
        }
    }
    if !batch.is_empty() {
        source
            .timeline_upsert_batch(&batch, PersistMode::Buffer)
            .map_err(|error| CommandError::internal(error.to_string()))?;
    }
    delete_stale_timeline(source, source_id, &current_ids)?;
    let snapshot = source.snapshot();
    if !source
        .timeline_counts_ready(&snapshot, source_id)
        .map_err(|error| CommandError::internal(error.to_string()))?
    {
        source
            .timeline_rebuild_count_indexes(source_id, PersistMode::Buffer)
            .map_err(|error| CommandError::internal(error.to_string()))?;
    }
    Ok(())
}

pub(super) fn mirror_timeline_ids(
    connection: &Connection,
    source: &ReadWriteSourceStorageHandle,
    source_id: &str,
    event_ids: &[String],
) -> Result<(), CommandError> {
    if event_ids.is_empty() {
        return Ok(());
    }
    let placeholders = (1..=event_ids.len())
        .map(|index| format!("?{index}"))
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "SELECT id, source_object_id, event_type, ts, title, description, parser_id,
                parser_version, confidence, source_attribution, attrs
         FROM timeline_events WHERE id IN ({placeholders})"
    );
    let mut statement = connection
        .prepare(&sql)
        .map_err(|error| CommandError::internal(error.to_string()))?;
    let rows = statement
        .query_map(rusqlite::params_from_iter(event_ids.iter()), |row| {
            timeline_record_from_row(row, source_id)
        })
        .map_err(|error| CommandError::internal(error.to_string()))?;
    let records = rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| CommandError::internal(error.to_string()))?;
    let existing_ids = records
        .iter()
        .map(|record| record.id.clone())
        .collect::<HashSet<_>>();
    for batch in records.chunks(BATCH_SIZE) {
        source
            .timeline_upsert_batch(batch, PersistMode::Buffer)
            .map_err(|error| CommandError::internal(error.to_string()))?;
    }
    let deleted = event_ids
        .iter()
        .filter(|id| !existing_ids.contains(*id))
        .cloned()
        .collect::<Vec<_>>();
    source
        .timeline_delete_batch(source_id, &deleted, PersistMode::Buffer)
        .map_err(|error| CommandError::internal(error.to_string()))?;
    Ok(())
}

fn timeline_record_from_row(
    row: &rusqlite::Row<'_>,
    source_id: &str,
) -> rusqlite::Result<TimelineRecord> {
    let attrs: String = row.get(10)?;
    Ok(TimelineRecord {
        id: row.get(0)?,
        data_source_id: source_id.to_owned(),
        source_object_id: row.get(1)?,
        event_type: row.get(2)?,
        timestamp: persistence_sqlite::util::parse_datetime(&row.get::<_, String>(3)?),
        title: row.get(4)?,
        description: row.get(5)?,
        parser_id: row.get(6)?,
        parser_version: row.get(7)?,
        confidence: row.get(8)?,
        source_attribution: row.get(9)?,
        attrs: serde_json::from_str::<BTreeMap<String, serde_json::Value>>(&attrs)
            .unwrap_or_default(),
    })
}

fn delete_stale_timeline(
    source: &ReadWriteSourceStorageHandle,
    source_id: &str,
    current_ids: &HashSet<String>,
) -> Result<(), CommandError> {
    let snapshot = source.snapshot();
    let mut cursor = None;
    loop {
        let page = source
            .timeline_list_by_time(
                &snapshot,
                source_id,
                cursor.as_deref(),
                persistence_fjall::ScanDirection::Forward,
                BATCH_SIZE,
            )
            .map_err(|error| CommandError::internal(error.to_string()))?;
        let stale = page
            .records
            .into_iter()
            .map(|record| record.id)
            .filter(|id| !current_ids.contains(id))
            .collect::<Vec<_>>();
        source
            .timeline_delete_batch(source_id, &stale, PersistMode::Buffer)
            .map_err(|error| CommandError::internal(error.to_string()))?;
        cursor = page.next_cursor;
        if cursor.is_none() {
            break;
        }
    }
    Ok(())
}

fn parse_optional_time(value: Option<String>) -> Option<chrono::DateTime<Utc>> {
    value.and_then(|value| persistence_sqlite::util::parse_opt_datetime(&value))
}
