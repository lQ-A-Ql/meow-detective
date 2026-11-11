use crate::detail::open_store_for_write;
use persistence_fjall::{
    ArtifactRecord, ArtifactRecordStore, PersistMode, TimelineRecord, TimelineRecordStore,
};
use persistence_sqlite::{DbError, DbResult};
use rusqlite::{Connection, Row};
use serde_json::Value;

const PAGE_SIZE: usize = 256;

pub fn merge(
    main: &Connection,
    staging: &Connection,
    _case_id: &str,
    source_id: &str,
) -> DbResult<Option<(u64, u64)>> {
    let Some(store) = open_store_for_write(main)? else {
        return Ok(None);
    };
    if crate::detail::source_id(main)?.as_deref() != Some(source_id) {
        return Err(DbError::System(
            "analysis staging source identity mismatch".into(),
        ));
    }
    let artifacts = ArtifactRecordStore::from_source_store((*store).clone());
    let timelines = TimelineRecordStore::from_source_store((*store).clone());
    let snapshot = staging.unchecked_transaction()?;
    let artifact_count = validate_artifacts(&snapshot, source_id)?;
    let timeline_count = validate_timelines(&snapshot, source_id)?;
    let mut batch = Vec::with_capacity(PAGE_SIZE);
    let mut statement = snapshot.prepare(
        "SELECT id, artifact_type, file_id, display_name, summary, data_json,
                extractor_id, extractor_version, confidence, source_attribution, created_at
         FROM artifact_rows ORDER BY id",
    )?;
    for row in statement.query_map([], |row| artifact(row, source_id))? {
        batch.push(row?);
        if batch.len() == PAGE_SIZE {
            artifacts
                .upsert_batch(&batch, PersistMode::Buffer)
                .map_err(storage_error)?;
            batch.clear();
        }
    }
    artifacts
        .upsert_batch(&batch, PersistMode::Buffer)
        .map_err(storage_error)?;
    let mut batch = Vec::with_capacity(PAGE_SIZE);
    let mut statement = snapshot.prepare(
        "SELECT id, file_id, event_type, timestamp, title, description,
                parser_id, parser_version, confidence, source_attribution, data_json
         FROM timeline_rows ORDER BY id",
    )?;
    for row in statement.query_map([], |row| timeline(row, source_id))? {
        batch.push(row?);
        if batch.len() == PAGE_SIZE {
            timelines
                .upsert_batch(&batch, PersistMode::Buffer)
                .map_err(storage_error)?;
            batch.clear();
        }
    }
    timelines
        .upsert_batch(&batch, PersistMode::Buffer)
        .map_err(storage_error)?;
    store
        .database()
        .persist(PersistMode::SyncAll)
        .map_err(storage_error)?;
    Ok(Some((artifact_count, timeline_count)))
}

fn validate_artifacts(conn: &Connection, source_id: &str) -> DbResult<u64> {
    let mut statement = conn.prepare(
        "SELECT id, artifact_type, file_id, display_name, summary, data_json,
                extractor_id, extractor_version, confidence, source_attribution, created_at
         FROM artifact_rows ORDER BY id",
    )?;
    let mut count = 0u64;
    for row in statement.query_map([], |row| artifact(row, source_id))? {
        row?;
        count += 1;
    }
    Ok(count)
}

fn validate_timelines(conn: &Connection, source_id: &str) -> DbResult<u64> {
    let mut statement = conn.prepare(
        "SELECT id, file_id, event_type, timestamp, title, description,
                parser_id, parser_version, confidence, source_attribution, data_json
         FROM timeline_rows ORDER BY id",
    )?;
    let mut count = 0u64;
    for row in statement.query_map([], |row| timeline(row, source_id))? {
        row?;
        count += 1;
    }
    Ok(count)
}

fn storage_error(error: persistence_fjall::FjallStorageError) -> DbError {
    DbError::System(error.to_string())
}

fn json(row: &Row<'_>, index: usize) -> rusqlite::Result<Value> {
    let text: String = row.get(index)?;
    serde_json::from_str(&text).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            index,
            rusqlite::types::Type::Text,
            Box::new(error),
        )
    })
}

fn artifact(row: &Row<'_>, source_id: &str) -> rusqlite::Result<ArtifactRecord> {
    Ok(ArtifactRecord {
        id: row.get(0)?,
        data_source_id: source_id.into(),
        artifact_type: row.get(1)?,
        source_object_id: row.get(2)?,
        content_sha256: None,
        attrs: serde_json::json!({
            "title": row.get::<_,String>(3)?, "summary": row.get::<_,String>(4)?,
            "attrs": json(row,5)?, "extractorId": row.get::<_,Option<String>>(6)?,
            "extractorVersion": row.get::<_,Option<String>>(7)?, "confidence": row.get::<_,Option<f32>>(8)?,
            "sourceAttribution": row.get::<_,Option<String>>(9)?, "createdAt": row.get::<_,String>(10)?,
        }),
    })
}

fn timeline(row: &Row<'_>, source_id: &str) -> rusqlite::Result<TimelineRecord> {
    let timestamp: String = row.get(3)?;
    let timestamp = chrono::DateTime::parse_from_rfc3339(&timestamp)
        .map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                3,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?
        .with_timezone(&chrono::Utc);
    let attrs = serde_json::from_value(json(row, 10)?).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(10, rusqlite::types::Type::Text, Box::new(error))
    })?;
    Ok(TimelineRecord {
        id: row.get(0)?,
        data_source_id: source_id.into(),
        source_object_id: row.get(1)?,
        event_type: row.get(2)?,
        timestamp,
        title: row.get(4)?,
        description: row.get(5)?,
        parser_id: row.get(6)?,
        parser_version: row.get(7)?,
        confidence: row.get(8)?,
        source_attribution: row.get(9)?,
        attrs,
    })
}
