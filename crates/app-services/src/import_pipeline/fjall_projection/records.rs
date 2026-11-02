use rusqlite::Connection;

use crate::storage_runtime::ReadWriteSourceStorageHandle;
use persistence_fjall::PersistMode;
use transport::CommandError;

const BATCH_SIZE: usize = 512;

pub(super) fn mirror_parser_tables(
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
    let rows = statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|error| CommandError::internal(error.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| CommandError::internal(error.to_string()))?;
    Ok(rows)
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
    let family = table.split('_').next().unwrap_or("parser");
    let mut batch = Vec::with_capacity(BATCH_SIZE);
    for row in rows {
        let (row_id, mut payload) =
            row.map_err(|error| CommandError::internal(error.to_string()))?;
        if let serde_json::Value::Object(fields) = &mut payload {
            fields.insert(
                "sourceTable".to_owned(),
                serde_json::Value::String(table.to_owned()),
            );
        }
        batch.push(persistence_fjall::ParserRecord {
            id: format!("table:{table}:{row_id}"),
            data_source_id: source_id.to_owned(),
            family: family.to_owned(),
            kind: parser_table_kind(table).to_owned(),
            status: "ready".to_owned(),
            summary_sha256: infrastructure::hashing::sha256_bytes(payload.to_string().as_bytes()),
            payload,
        });
        if batch.len() == BATCH_SIZE {
            source
                .parser_upsert_batch(&batch, PersistMode::Buffer)
                .map_err(|error| CommandError::internal(error.to_string()))?;
            batch.clear();
        }
    }
    if !batch.is_empty() {
        source
            .parser_upsert_batch(&batch, PersistMode::Buffer)
            .map_err(|error| CommandError::internal(error.to_string()))?;
    }
    Ok(())
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

pub(super) fn mirror_linux_parser_record(
    connection: &Connection,
    source: &ReadWriteSourceStorageHandle,
    source_id: &str,
) -> Result<(), CommandError> {
    let facts = connection.query_row(
        "SELECT facts_json FROM linux_evidence_facts WHERE data_source_id = ?1 AND schema_version = 1",
        [source_id],
        |row| row.get::<_, String>(0),
    );
    let Ok(facts) = facts else { return Ok(()) };
    let payload = serde_json::from_str::<serde_json::Value>(&facts)
        .map_err(|error| CommandError::internal(error.to_string()))?;
    let record = persistence_fjall::ParserRecord {
        id: "linux:evidence-facts".to_owned(),
        data_source_id: source_id.to_owned(),
        family: "linux".to_owned(),
        kind: "evidence_facts".to_owned(),
        status: "ready".to_owned(),
        summary_sha256: infrastructure::hashing::sha256_bytes(payload.to_string().as_bytes()),
        payload,
    };
    source
        .parser_upsert(&record, PersistMode::Buffer)
        .map_err(|error| CommandError::internal(error.to_string()))
}

pub(super) fn mirror_android_parser_records(
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
                "field": row.get::<_, String>(1)?, "value": row.get::<_, String>(2)?,
                "confidence": row.get::<_, String>(3)?, "sourceFileId": row.get::<_, String>(4)?,
                "sourcePath": row.get::<_, String>(5)?, "parser": row.get::<_, String>(6)?,
                "warning": row.get::<_, Option<String>>(7)?,
            });
            Ok(parser_record(
                format!("android-fact:{id}"),
                source_id,
                "system_fact",
                payload,
            ))
        })
        .map_err(|error| CommandError::internal(error.to_string()))?;
    flush_parser_rows(source, rows)?;

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
            Ok(parser_record(format!("android-package:{id}"), source_id, "package", payload))
        })
        .map_err(|error| CommandError::internal(error.to_string()))?;
    flush_parser_rows(source, rows)
}

fn parser_record(
    id: String,
    source_id: &str,
    kind: &str,
    payload: serde_json::Value,
) -> persistence_fjall::ParserRecord {
    persistence_fjall::ParserRecord {
        id,
        data_source_id: source_id.to_owned(),
        family: "android".to_owned(),
        kind: kind.to_owned(),
        status: "ready".to_owned(),
        summary_sha256: infrastructure::hashing::sha256_bytes(payload.to_string().as_bytes()),
        payload,
    }
}

fn flush_parser_rows<I>(source: &ReadWriteSourceStorageHandle, rows: I) -> Result<(), CommandError>
where
    I: Iterator<Item = rusqlite::Result<persistence_fjall::ParserRecord>>,
{
    let mut batch = Vec::with_capacity(BATCH_SIZE);
    for row in rows {
        batch.push(row.map_err(|error| CommandError::internal(error.to_string()))?);
        if batch.len() == BATCH_SIZE {
            source
                .parser_upsert_batch(&batch, PersistMode::Buffer)
                .map_err(|error| CommandError::internal(error.to_string()))?;
            batch.clear();
        }
    }
    if !batch.is_empty() {
        source
            .parser_upsert_batch(&batch, PersistMode::Buffer)
            .map_err(|error| CommandError::internal(error.to_string()))?;
    }
    Ok(())
}
