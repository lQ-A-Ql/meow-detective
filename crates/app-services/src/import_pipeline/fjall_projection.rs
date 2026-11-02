use artifact_storage::ArtifactStore;
use chrono::Utc;
use persistence_fjall::{
    encode_key, ArtifactRecord, CatalogEntryType, FileEntryRecord, KeyPart, PersistMode,
    TimelineRecord, VersionedEnvelope,
};
use rusqlite::Connection;
use std::collections::BTreeMap;

use super::context::ImportJobContext;
use crate::storage_runtime::ReadWriteSourceStorageHandle;
use transport::CommandError;

mod nested;
use nested::mirror_nested_lineage;

pub(super) fn mirror_source_records(
    ctx: &ImportJobContext<'_>,
    source_id: &str,
) -> Result<String, CommandError> {
    mirror_source_records_from_connections(
        ctx.conn,
        ctx.case_root,
        &ctx.case_id.0,
        ctx.source_connection()?,
        source_id,
    )
}

pub(crate) fn mirror_source_records_from_connections(
    case_connection: &Connection,
    case_root: &std::path::Path,
    case_id: &str,
    source_connection: &Connection,
    source_id: &str,
) -> Result<String, CommandError> {
    let source = crate::source_db::open_registered_fjall_source_read_write(
        case_root,
        &domain::DataSourceId(source_id.to_owned()),
    )
    .map_err(|_| CommandError::internal("Fjall source storage is unavailable"))?;
    mirror_catalog(source_connection, &source, source_id)?;
    mirror_timeline(source_connection, &source, source_id)?;
    mirror_artifacts(
        case_connection,
        case_root,
        case_id,
        source_connection,
        &source,
        source_id,
    )?;
    mirror_android_parser_records(source_connection, &source, source_id)?;
    mirror_linux_parser_record(source_connection, &source, source_id)?;
    mirror_parser_tables(source_connection, &source, source_id)?;
    mirror_nested_lineage(case_connection, &source, source_id)?;
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

fn mirror_parser_tables(
    connection: &Connection,
    source: &ReadWriteSourceStorageHandle,
    source_id: &str,
) -> Result<(), CommandError> {
    let tables = parser_table_names(connection)?;
    for table in tables {
        mirror_parser_table(connection, source, source_id, &table)?;
    }
    Ok(())
}

fn parser_table_names(connection: &Connection) -> Result<Vec<String>, CommandError> {
    let mut statement = connection
        .prepare(
            "SELECT name FROM sqlite_master
             WHERE type = 'table' AND name NOT LIKE 'sqlite_%'
               AND (name LIKE 'android_%' OR name LIKE 'linux_%' OR name LIKE 'ceph_%'
                    OR name LIKE 'nested_%' OR name LIKE 'correlation_%')
             ORDER BY name ASC",
        )
        .map_err(|error| CommandError::internal(error.to_string()))?;
    let result = statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|error| CommandError::internal(error.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| CommandError::internal(error.to_string()));
    result
}

fn parser_table_kind(table: &str) -> &'static str {
    match table {
        "ceph_fs_metadata_objects" => "ceph_fs_metadata_objects",
        "ceph_bluefs_files" => "ceph_bluefs_files",
        "ceph_rocksdb_latest_state" => "ceph_rocksdb_latest_state",
        _ => "parser_table",
    }
}

fn mirror_parser_table(
    connection: &Connection,
    source: &ReadWriteSourceStorageHandle,
    source_id: &str,
    table: &str,
) -> Result<(), CommandError> {
    let quoted = format!("\"{}\"", table.replace('"', "\"\""));
    let mut statement = connection
        .prepare(&format!("SELECT rowid, * FROM {quoted} ORDER BY rowid ASC"))
        .map_err(|error| CommandError::internal(error.to_string()))?;
    let columns = statement
        .column_names()
        .iter()
        .map(|value| value.to_string())
        .collect::<Vec<_>>();
    let rows = statement
        .query_map([], |row| {
            let row_id: i64 = row.get(0)?;
            let mut payload = serde_json::Map::new();
            for (index, column) in columns.iter().enumerate().skip(1) {
                payload.insert(column.to_string(), sqlite_value(row.get_ref(index)?));
            }
            Ok((row_id, serde_json::Value::Object(payload)))
        })
        .map_err(|error| CommandError::internal(error.to_string()))?;
    let family = parser_family(table);
    for row in rows {
        let (row_id, payload) = row.map_err(|error| CommandError::internal(error.to_string()))?;
        let mut payload = payload;
        if let serde_json::Value::Object(fields) = &mut payload {
            fields.insert(
                "sourceTable".to_owned(),
                serde_json::Value::String(table.to_owned()),
            );
        }
        let record = persistence_fjall::ParserRecord {
            id: format!("table:{table}:{row_id}"),
            data_source_id: source_id.to_owned(),
            family: family.to_owned(),
            kind: parser_table_kind(table).to_owned(),
            status: "ready".to_owned(),
            summary_sha256: infrastructure::hashing::sha256_bytes(payload.to_string().as_bytes()),
            payload,
        };
        source
            .parser_upsert(&record, PersistMode::Buffer)
            .map_err(|error| CommandError::internal(error.to_string()))?;
    }
    Ok(())
}

fn parser_family(table: &str) -> &str {
    table.split('_').next().unwrap_or("parser")
}

fn sqlite_value(value: rusqlite::types::ValueRef<'_>) -> serde_json::Value {
    match value {
        rusqlite::types::ValueRef::Null => serde_json::Value::Null,
        rusqlite::types::ValueRef::Integer(value) => value.into(),
        rusqlite::types::ValueRef::Real(value) => serde_json::json!(value),
        rusqlite::types::ValueRef::Text(value) => {
            String::from_utf8_lossy(value).into_owned().into()
        }
        rusqlite::types::ValueRef::Blob(value) => hex::encode(value).into(),
    }
}

fn mirror_linux_parser_record(
    connection: &Connection,
    source: &ReadWriteSourceStorageHandle,
    source_id: &str,
) -> Result<(), CommandError> {
    let facts = connection
        .query_row(
            "SELECT facts_json FROM linux_evidence_facts WHERE data_source_id = ?1 AND schema_version = 1",
            [source_id],
            |row| row.get::<_, String>(0),
        );
    let Ok(facts) = facts else { return Ok(()) };
    let payload = serde_json::from_str::<serde_json::Value>(&facts)
        .map_err(|error| CommandError::internal(error.to_string()))?;
    let summary = infrastructure::hashing::sha256_bytes(payload.to_string().as_bytes());
    let record = persistence_fjall::ParserRecord {
        id: "linux:evidence-facts".to_owned(),
        data_source_id: source_id.to_owned(),
        family: "linux".to_owned(),
        kind: "evidence_facts".to_owned(),
        status: "ready".to_owned(),
        summary_sha256: summary,
        payload,
    };
    source
        .parser_upsert(&record, PersistMode::Buffer)
        .map_err(|error| CommandError::internal(error.to_string()))?;
    Ok(())
}

fn mirror_android_parser_records(
    connection: &Connection,
    source: &ReadWriteSourceStorageHandle,
    source_id: &str,
) -> Result<(), CommandError> {
    let mut facts = connection
        .prepare(
            "SELECT id, field_key, field_value, confidence, source_file_id, source_path, parser, warning
             FROM android_system_facts WHERE data_source_id = ?1 ORDER BY id ASC",
        )
        .map_err(|error| CommandError::internal(error.to_string()))?;
    let rows = facts
        .query_map([source_id], |row| {
            let id: String = row.get(0)?;
            let payload = serde_json::json!({
                "field": row.get::<_, String>(1)?,
                "value": row.get::<_, String>(2)?,
                "confidence": row.get::<_, String>(3)?,
                "sourceFileId": row.get::<_, String>(4)?,
                "sourcePath": row.get::<_, String>(5)?,
                "parser": row.get::<_, String>(6)?,
                "warning": row.get::<_, Option<String>>(7)?,
            });
            Ok(persistence_fjall::ParserRecord {
                id: format!("android-fact:{id}"),
                data_source_id: source_id.to_owned(),
                family: "android".to_owned(),
                kind: "system_fact".to_owned(),
                status: "ready".to_owned(),
                summary_sha256: infrastructure::hashing::sha256_bytes(
                    payload.to_string().as_bytes(),
                ),
                payload,
            })
        })
        .map_err(|error| CommandError::internal(error.to_string()))?;
    for row in rows {
        source
            .parser_upsert(
                &row.map_err(|error| CommandError::internal(error.to_string()))?,
                PersistMode::Buffer,
            )
            .map_err(|error| CommandError::internal(error.to_string()))?;
    }
    let mut packages = connection
        .prepare(
            "SELECT id, package_name, app_name, version_code, install_time, update_time,
                    installer, uid, code_path, source_file_id, source_path, parser, warning
             FROM android_packages WHERE data_source_id = ?1 ORDER BY id ASC",
        )
        .map_err(|error| CommandError::internal(error.to_string()))?;
    let rows = packages
        .query_map([source_id], |row| {
            let id: String = row.get(0)?;
            let payload = serde_json::json!({
                "packageName": row.get::<_, String>(1)?, "appName": row.get::<_, Option<String>>(2)?,
                "versionCode": row.get::<_, Option<String>>(3)?, "installTime": row.get::<_, Option<String>>(4)?,
                "updateTime": row.get::<_, Option<String>>(5)?, "installer": row.get::<_, Option<String>>(6)?,
                "uid": row.get::<_, Option<u32>>(7)?, "codePath": row.get::<_, Option<String>>(8)?,
                "sourceFileId": row.get::<_, String>(9)?, "sourcePath": row.get::<_, String>(10)?,
                "parser": row.get::<_, String>(11)?, "warning": row.get::<_, Option<String>>(12)?,
            });
            Ok(persistence_fjall::ParserRecord {
                id: format!("android-package:{id}"), data_source_id: source_id.to_owned(),
                family: "android".to_owned(), kind: "package".to_owned(), status: "ready".to_owned(),
                summary_sha256: infrastructure::hashing::sha256_bytes(payload.to_string().as_bytes()), payload,
            })
        })
        .map_err(|error| CommandError::internal(error.to_string()))?;
    for row in rows {
        source
            .parser_upsert(
                &row.map_err(|error| CommandError::internal(error.to_string()))?,
                PersistMode::Buffer,
            )
            .map_err(|error| CommandError::internal(error.to_string()))?;
    }
    Ok(())
}

fn mirror_artifacts(
    case_connection: &Connection,
    case_root: &std::path::Path,
    case_id: &str,
    connection: &Connection,
    source: &ReadWriteSourceStorageHandle,
    source_id: &str,
) -> Result<(), CommandError> {
    let object_store =
        ArtifactStore::new(case_root).map_err(|error| CommandError::internal(error.to_string()))?;
    let mut statement = connection
        .prepare(
            "SELECT id, artifact_type, source_object_id, summary, attrs,
                    title, summary, extractor_id, extractor_version, confidence,
                    source_attribution, created_at
             FROM artifacts WHERE data_source_id = ?1 ORDER BY id ASC",
        )
        .map_err(|error| CommandError::internal(error.to_string()))?;
    let rows = statement
        .query_map([source_id], |row| {
            let summary: String = row.get(3)?;
            let attrs: String = row.get(4)?;
            let payload = serde_json::json!({
                "attrs": serde_json::from_str::<serde_json::Value>(&attrs)
                    .unwrap_or(serde_json::Value::Null),
                "title": row.get::<_, String>(5)?,
                "summary": row.get::<_, String>(6)?,
                "extractorId": row.get::<_, Option<String>>(7)?,
                "extractorVersion": row.get::<_, Option<String>>(8)?,
                "confidence": row.get::<_, Option<f32>>(9)?,
                "sourceAttribution": row.get::<_, Option<String>>(10)?,
                "createdAt": row.get::<_, String>(11)?,
            });
            Ok((
                row.get::<_, String>(0)?,
                ArtifactRecord {
                    id: row.get(0)?,
                    data_source_id: source_id.to_owned(),
                    artifact_type: row.get(1)?,
                    source_object_id: row.get(2)?,
                    content_sha256: Some(infrastructure::hashing::sha256_bytes(summary.as_bytes())),
                    attrs: payload,
                },
            ))
        })
        .map_err(|error| CommandError::internal(error.to_string()))?;
    for row in rows {
        let (artifact_id, record) =
            row.map_err(|error| CommandError::internal(error.to_string()))?;
        source
            .artifact_upsert(&record, PersistMode::Buffer)
            .map_err(|error| CommandError::internal(error.to_string()))?;
        let object_bytes = serde_json::to_vec(&record.attrs)
            .map_err(|error| CommandError::internal(error.to_string()))?;
        let object = crate::artifact_storage_service::write_object(
            case_connection,
            &object_store,
            case_id,
            &object_bytes,
            "application/json",
        )
        .map_err(|error| CommandError::internal(error.to_string()))?;
        crate::artifact_storage_service::create_reference(
            case_connection,
            case_id,
            &object.object_id,
            Some(source_id),
            "artifact",
            &artifact_id,
            "metadata",
        )
        .map_err(|error| CommandError::internal(error.to_string()))?;
    }
    Ok(())
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
