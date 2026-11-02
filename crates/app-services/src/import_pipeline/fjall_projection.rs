use chrono::Utc;
use persistence_fjall::{
    encode_key, CatalogEntryType, FileEntryRecord, KeyPart, PersistMode, TimelineRecord,
    VersionedEnvelope,
};
use rusqlite::Connection;
use std::collections::BTreeMap;

use super::context::ImportJobContext;
use crate::storage_runtime::ReadWriteSourceStorageHandle;
use transport::CommandError;

pub(super) fn mirror_source_records(
    ctx: &ImportJobContext<'_>,
    source_id: &str,
) -> Result<String, CommandError> {
    let source = crate::source_db::open_registered_fjall_source_read_write(
        ctx.case_root,
        &domain::DataSourceId(source_id.to_owned()),
    )
    .map_err(|_| CommandError::internal("Fjall source storage is unavailable"))?;
    mirror_catalog(ctx.source_connection()?, &source, source_id)?;
    mirror_timeline(ctx.source_connection()?, &source, source_id)?;
    source
        .persist(PersistMode::SyncAll)
        .map_err(|error| CommandError::internal(error.to_string()))?;
    let marker = VersionedEnvelope::new(
        "source_projection_complete",
        1,
        format!("projection:{source_id}"),
        source_id,
        1,
        source_id.as_bytes().to_vec(),
    );
    let receipt = source
        .write_envelope(
            persistence_fjall::KeyspaceName::Meta,
            encode_key("projection", &[KeyPart::Text(source_id.to_owned())])
                .map_err(|error| CommandError::internal(error.to_string()))?,
            &marker,
            PersistMode::SyncAll,
        )
        .map_err(|error| CommandError::internal(error.to_string()))?;
    Ok(hex::encode(receipt.batch_digest))
}

fn mirror_catalog(
    connection: &Connection,
    source: &ReadWriteSourceStorageHandle,
    source_id: &str,
) -> Result<(), CommandError> {
    let mut statement = connection
        .prepare(
            "SELECT id, parent_id, data_source_id, path, name, entry_type, size, ext,
                    deleted, hidden, system, created_at, modified_at, accessed_at, changed_at,
                    hash_sha256, encrypted, read_only, archive, unix_mode, partition_index
             FROM file_entries WHERE data_source_id = ?1 ORDER BY id ASC",
        )
        .map_err(|error| CommandError::internal(error.to_string()))?;
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
    for row in rows {
        let record = row.map_err(|error| CommandError::internal(error.to_string()))?;
        source
            .catalog_upsert(&record, PersistMode::Buffer)
            .map_err(|error| CommandError::internal(error.to_string()))?;
    }
    Ok(())
}

fn mirror_timeline(
    connection: &Connection,
    source: &ReadWriteSourceStorageHandle,
    source_id: &str,
) -> Result<(), CommandError> {
    let mut statement = connection
        .prepare(
            "SELECT id, source_object_id, event_type, ts, title, description, parser_id,
                    parser_version, confidence, source_attribution, attrs
             FROM timeline_events ORDER BY ts DESC, id ASC",
        )
        .map_err(|error| CommandError::internal(error.to_string()))?;
    let rows = statement
        .query_map([], |row| {
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
        })
        .map_err(|error| CommandError::internal(error.to_string()))?;
    for row in rows {
        let record = row.map_err(|error| CommandError::internal(error.to_string()))?;
        source
            .timeline_upsert(&record, PersistMode::Buffer)
            .map_err(|error| CommandError::internal(error.to_string()))?;
    }
    Ok(())
}

fn parse_optional_time(value: Option<String>) -> Option<chrono::DateTime<Utc>> {
    value.and_then(|value| persistence_sqlite::util::parse_opt_datetime(&value))
}
