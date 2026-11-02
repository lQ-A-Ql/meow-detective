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
    mirror_artifacts(ctx.source_connection()?, &source, source_id)?;
    mirror_android_parser_records(ctx.source_connection()?, &source, source_id)?;
    mirror_linux_parser_record(ctx.source_connection()?, &source, source_id)?;
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
    let summary = infrastructure::hashing::sha256_bytes(facts.as_bytes());
    let record = persistence_fjall::ParserRecord {
        id: "linux:evidence-facts".to_owned(),
        data_source_id: source_id.to_owned(),
        family: "linux".to_owned(),
        kind: "evidence_facts".to_owned(),
        status: "ready".to_owned(),
        summary_sha256: summary,
        payload: serde_json::from_str(&facts).unwrap_or(serde_json::Value::Null),
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
            Ok(persistence_fjall::ParserRecord {
                id: format!("android-fact:{id}"),
                data_source_id: source_id.to_owned(),
                family: "android".to_owned(),
                kind: "system_fact".to_owned(),
                status: "ready".to_owned(),
                summary_sha256: infrastructure::hashing::sha256_bytes(
                    format!(
                        "{}:{}:{}",
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?
                    )
                    .as_bytes(),
                ),
                payload: serde_json::json!({
                    "field": row.get::<_, String>(1)?,
                    "value": row.get::<_, String>(2)?,
                    "confidence": row.get::<_, String>(3)?,
                    "sourceFileId": row.get::<_, String>(4)?,
                    "sourcePath": row.get::<_, String>(5)?,
                    "parser": row.get::<_, String>(6)?,
                    "warning": row.get::<_, Option<String>>(7)?,
                }),
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
    connection: &Connection,
    source: &ReadWriteSourceStorageHandle,
    source_id: &str,
) -> Result<(), CommandError> {
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
            Ok(ArtifactRecord {
                id: row.get(0)?,
                data_source_id: source_id.to_owned(),
                artifact_type: row.get(1)?,
                source_object_id: row.get(2)?,
                content_sha256: Some(infrastructure::hashing::sha256_bytes(summary.as_bytes())),
                attrs: payload,
            })
        })
        .map_err(|error| CommandError::internal(error.to_string()))?;
    for row in rows {
        let record = row.map_err(|error| CommandError::internal(error.to_string()))?;
        source
            .artifact_upsert(&record, PersistMode::Buffer)
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
