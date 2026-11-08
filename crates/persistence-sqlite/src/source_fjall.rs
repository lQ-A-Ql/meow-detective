use domain::{Artifact, DataSourceId, EntryType, FileEntry, FileEntryId, TimelineEvent};
use persistence_fjall::{
    ArtifactRecord, CatalogEntryType, CatalogRecordStore, DatabaseConfig, FileEntryRecord,
    PersistMode, SourceStore, TimelineRecord,
};
use rusqlite::Connection;
use std::sync::Arc;
pub(crate) mod analysis_staging;
mod catalog_staging;
mod fingerprints;
pub use catalog_staging::merge_staging_catalog;

const FJALL_ROOT_KEY: &str = "fjall_root";
const SOURCE_ID_KEY: &str = "data_source_id";

pub fn configure_source_fjall(
    conn: &Connection,
    source_db: &std::path::Path,
) -> crate::DbResult<()> {
    let file_name = source_db
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    let root_name = file_name
        .strip_prefix("source.db.build.")
        .map(|attempt| format!("source.fjall.build.{attempt}"))
        .unwrap_or_else(|| "source.fjall".to_owned());
    conn.execute(
        "INSERT INTO source_meta(key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO NOTHING",
        rusqlite::params![FJALL_ROOT_KEY, root_name],
    )?;
    Ok(())
}

pub fn configure_source_identity(conn: &Connection, source_id: &str) -> crate::DbResult<()> {
    if let Some(owner) = self::source_id(conn)? {
        if owner != source_id {
            return Err(crate::DbError::System(
                "source identity cannot be replaced".into(),
            ));
        }
        return Ok(());
    }
    conn.execute(
        "INSERT INTO source_meta(key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        rusqlite::params![SOURCE_ID_KEY, source_id],
    )?;
    Ok(())
}

pub fn bind_generation(conn: &Connection, name: &str) -> crate::DbResult<()> {
    if name != "source.fjall" && !name.starts_with("source.fjall.build.") {
        return Err(crate::DbError::System(
            "invalid source generation name".into(),
        ));
    }
    if name.contains(['/', '\\', ':']) || name.ends_with('.') {
        return Err(crate::DbError::System(
            "invalid source generation component".into(),
        ));
    }
    conn.execute(
        "UPDATE source_meta SET value = ?1 WHERE key = ?2",
        rusqlite::params![name, FJALL_ROOT_KEY],
    )?;
    Ok(())
}

pub fn source_id(conn: &Connection) -> crate::DbResult<Option<String>> {
    match conn.query_row(
        "SELECT value FROM source_meta WHERE key = ?1",
        [SOURCE_ID_KEY],
        |row| row.get(0),
    ) {
        Ok(value) => Ok(Some(value)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(error) => {
            let missing_table = matches!(
                &error,
                rusqlite::Error::SqliteFailure(code, message)
                    if code.extended_code == rusqlite::ffi::SQLITE_ERROR
                        && message.as_deref().is_some_and(|value| value.contains("no such table"))
            );
            if missing_table {
                Ok(None)
            } else {
                Err(error.into())
            }
        }
    }
}

fn require_owner(conn: &Connection, requested: &str) -> crate::DbResult<()> {
    if source_id(conn)?.as_deref() != Some(requested) {
        return Err(crate::DbError::System(
            "source detail owner mismatch".into(),
        ));
    }
    Ok(())
}

pub fn partition_index(conn: &Connection, record_id: &str) -> crate::DbResult<Option<usize>> {
    let Some(store) = open_store(conn)? else {
        return Ok(None);
    };
    let repository = CatalogRecordStore::from_source_store((*store).clone());
    let snapshot = repository.read_snapshot();
    let owner = source_id(conn)?
        .ok_or_else(|| crate::DbError::System("source identity is missing".into()))?;
    let Some(record) = repository
        .get(&snapshot, &owner, record_id)
        .map_err(|error| crate::DbError::System(error.to_string()))?
    else {
        return Ok(None);
    };
    record
        .partition_index
        .map(|index| {
            usize::try_from(index).map_err(|_| {
                crate::DbError::System("partition index exceeds platform range".into())
            })
        })
        .transpose()
}

pub fn set_partition_index(
    conn: &Connection,
    record_id: &str,
    partition: usize,
) -> crate::DbResult<bool> {
    let Some(store) = open_store_for_write(conn)? else {
        return Ok(false);
    };
    let repository = CatalogRecordStore::from_source_store((*store).clone());
    let snapshot = repository.read_snapshot();
    let owner = source_id(conn)?
        .ok_or_else(|| crate::DbError::System("source identity is missing".into()))?;
    let Some(mut record) = repository
        .get(&snapshot, &owner, record_id)
        .map_err(|error| crate::DbError::System(error.to_string()))?
    else {
        return Ok(false);
    };
    record.partition_index = Some(
        u32::try_from(partition)
            .map_err(|_| crate::DbError::System("partition index exceeds Fjall range".into()))?,
    );
    repository
        .upsert(&record, persistence_fjall::PersistMode::SyncAll)
        .map_err(|error| crate::DbError::System(error.to_string()))?;
    Ok(true)
}

pub fn update_catalog_path(
    conn: &Connection,
    record_id: &str,
    path: &str,
    source_id: &str,
) -> crate::DbResult<bool> {
    let Some(store) = open_store_for_write(conn)? else {
        return Ok(false);
    };
    let repository = CatalogRecordStore::from_source_store((*store).clone());
    let snapshot = repository.read_snapshot();
    let Some(mut record) = repository
        .get(&snapshot, source_id, record_id)
        .map_err(|error| crate::DbError::System(error.to_string()))?
    else {
        return Ok(false);
    };
    record.path = path.to_owned();
    repository
        .upsert(&record, persistence_fjall::PersistMode::SyncAll)
        .map_err(|error| crate::DbError::System(error.to_string()))?;
    Ok(true)
}

pub fn update_catalog_parent(
    conn: &Connection,
    record_id: &str,
    parent_id: Option<String>,
    source_id: &str,
) -> crate::DbResult<bool> {
    let Some(store) = open_store_for_write(conn)? else {
        return Ok(false);
    };
    let repository = CatalogRecordStore::from_source_store((*store).clone());
    let snapshot = repository.read_snapshot();
    let Some(mut record) = repository
        .get(&snapshot, source_id, record_id)
        .map_err(|error| crate::DbError::System(error.to_string()))?
    else {
        return Ok(false);
    };
    record.parent_id = parent_id;
    repository
        .upsert(&record, persistence_fjall::PersistMode::SyncAll)
        .map_err(|error| crate::DbError::System(error.to_string()))?;
    Ok(true)
}

pub fn has_source_store(conn: &Connection) -> crate::DbResult<bool> {
    Ok(open_store(conn)?.is_some())
}

pub fn open_store(conn: &Connection) -> crate::DbResult<Option<Arc<SourceStore>>> {
    let root: Option<String> = match conn.query_row(
        "SELECT value FROM source_meta WHERE key = ?1",
        [FJALL_ROOT_KEY],
        |row| row.get(0),
    ) {
        Ok(value) => Some(value),
        Err(rusqlite::Error::QueryReturnedNoRows) => None,
        Err(error) => {
            let missing_table = matches!(
                &error,
                rusqlite::Error::SqliteFailure(code, message)
                    if code.extended_code == rusqlite::ffi::SQLITE_ERROR
                        && message.as_deref().is_some_and(|value| value.contains("no such table"))
            );
            if missing_table {
                None
            } else {
                return Err(error.into());
            }
        }
    };
    let Some(name) = root else { return Ok(None) };
    if name != "source.fjall" && !name.starts_with("source.fjall.build.") {
        return Err(crate::DbError::System(
            "invalid source generation name".into(),
        ));
    }
    if name.contains(['/', '\\', ':']) || name.ends_with('.') {
        return Err(crate::DbError::System(
            "invalid source generation component".into(),
        ));
    }
    let database_path: String = conn.query_row(
        "SELECT file FROM pragma_database_list WHERE name = 'main'",
        [],
        |row| row.get(0),
    )?;
    let parent = std::path::Path::new(&database_path)
        .parent()
        .ok_or_else(|| crate::DbError::System("source database has no directory".into()))?;
    let path = parent.join(name);
    if !path.is_dir() {
        let query_only: bool = conn.pragma_query_value(None, "query_only", |row| row.get(0))?;
        if query_only || conn.is_readonly(rusqlite::DatabaseName::Main)? {
            return Err(crate::DbError::System(
                "source generation directory is missing".into(),
            ));
        }
        std::fs::create_dir_all(&path)?;
    }
    persistence_fjall::open_shared_source_store(path, DatabaseConfig::default())
        .map(Some)
        .map_err(|error| crate::DbError::System(error.to_string()))
}

pub fn open_store_for_write(conn: &Connection) -> crate::DbResult<Option<Arc<SourceStore>>> {
    let query_only: bool = conn.pragma_query_value(None, "query_only", |row| row.get(0))?;
    if query_only || conn.is_readonly(rusqlite::DatabaseName::Main)? {
        return Err(crate::DbError::Io(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "source connection is read-only",
        )));
    }
    open_store(conn)
}

pub(crate) fn file_record(entry: &FileEntry) -> FileEntryRecord {
    FileEntryRecord {
        id: entry.id.0.clone(),
        parent_id: entry.parent_id.as_ref().map(|id| id.0.clone()),
        data_source_id: entry.data_source_id.0.clone(),
        path: entry.path.clone(),
        name: entry.name.clone(),
        entry_type: match entry.entry_type {
            EntryType::File => CatalogEntryType::File,
            EntryType::Directory => CatalogEntryType::Directory,
        },
        size: entry.size,
        ext: entry.ext.clone(),
        deleted: entry.deleted,
        hidden: entry.hidden,
        system: entry.system,
        read_only: entry.read_only,
        archive: entry.archive,
        encrypted: Some(entry.encrypted),
        created: entry.created_at,
        modified: entry.modified_at,
        accessed: entry.accessed_at,
        changed: entry.changed_at,
        hash_sha256: entry.hash_sha256.clone(),
        partition_index: None,
        unix_mode: entry.unix_mode,
    }
}

pub(crate) fn file_entry(record: FileEntryRecord) -> FileEntry {
    FileEntry {
        id: FileEntryId(record.id),
        parent_id: record.parent_id.map(FileEntryId),
        data_source_id: DataSourceId(record.data_source_id),
        path: record.path,
        name: record.name,
        entry_type: match record.entry_type {
            CatalogEntryType::File => EntryType::File,
            CatalogEntryType::Directory => EntryType::Directory,
        },
        size: record.size,
        ext: record.ext,
        deleted: record.deleted,
        hidden: record.hidden,
        system: record.system,
        encrypted: record.encrypted.unwrap_or(true),
        read_only: record.read_only,
        archive: record.archive,
        unix_mode: record.unix_mode,
        created_at: record.created,
        modified_at: record.modified,
        accessed_at: record.accessed,
        changed_at: record.changed,
        hash_sha256: record.hash_sha256,
    }
}

pub fn artifact_record(artifact: &Artifact, source_id: &str) -> ArtifactRecord {
    ArtifactRecord {
        id: artifact.id.0.clone(),
        data_source_id: source_id.to_owned(),
        artifact_type: artifact.family.clone(),
        source_object_id: artifact.source_object_id.as_ref().map(|id| id.0.clone()),
        content_sha256: None,
        attrs: serde_json::json!({
            "title": artifact.title,
            "summary": artifact.summary,
            "extractorId": artifact.extractor_id,
            "extractorVersion": artifact.extractor_version,
            "confidence": artifact.confidence,
            "sourceAttribution": artifact.source_attribution,
            "createdAt": artifact.created_at.to_rfc3339(),
            "attrs": artifact.attrs,
        }),
    }
}

pub fn timeline_record(event: &TimelineEvent, source_id: &str) -> TimelineRecord {
    TimelineRecord {
        id: event.id.0.clone(),
        data_source_id: source_id.to_owned(),
        source_object_id: event.source_object_id.clone(),
        event_type: event.event_type.clone(),
        timestamp: event.timestamp,
        title: event.title.clone(),
        description: event.description.clone(),
        parser_id: event.parser_id.clone(),
        parser_version: event.parser_version.clone(),
        confidence: event.confidence,
        source_attribution: event.source_attribution.clone(),
        attrs: event.attrs.clone(),
    }
}

pub(crate) fn upsert_files(conn: &Connection, entries: &[FileEntry]) -> crate::DbResult<bool> {
    let Some(store) = open_store_for_write(conn)? else {
        return Ok(false);
    };
    for entry in entries {
        require_owner(conn, &entry.data_source_id.0)?;
    }
    let records = entries.iter().map(file_record).collect::<Vec<_>>();
    CatalogRecordStore::from_source_store((*store).clone())
        .upsert_batch(&records, PersistMode::SyncAll)
        .map_err(|error| crate::DbError::System(error.to_string()))?;
    fingerprints::files(conn, entries)?;
    Ok(true)
}

pub fn upsert_files_with_partition(
    conn: &Connection,
    entries: &[FileEntry],
    partition_index: usize,
) -> crate::DbResult<bool> {
    let Some(store) = open_store_for_write(conn)? else {
        return Ok(false);
    };
    for entry in entries {
        require_owner(conn, &entry.data_source_id.0)?;
    }
    let partition_index = u32::try_from(partition_index)
        .map_err(|_| crate::DbError::System("partition index exceeds Fjall range".into()))?;
    let records = entries
        .iter()
        .map(|entry| {
            let mut record = file_record(entry);
            record.partition_index = Some(partition_index);
            record
        })
        .collect::<Vec<_>>();
    CatalogRecordStore::from_source_store((*store).clone())
        .upsert_batch(&records, persistence_fjall::PersistMode::SyncAll)
        .map_err(|error| crate::DbError::System(error.to_string()))?;
    fingerprints::files(conn, entries)?;
    Ok(true)
}

pub(crate) fn upsert_artifacts(
    conn: &Connection,
    artifacts: &[Artifact],
    source_id: &str,
) -> crate::DbResult<bool> {
    let Some(store) = open_store_for_write(conn)? else {
        return Ok(false);
    };
    require_owner(conn, source_id)?;
    let records = artifacts
        .iter()
        .map(|artifact| artifact_record(artifact, source_id))
        .collect::<Vec<_>>();
    persistence_fjall::ArtifactRecordStore::from_source_store((*store).clone())
        .upsert_batch(&records, PersistMode::SyncAll)
        .map_err(|error| crate::DbError::System(error.to_string()))?;
    Ok(true)
}

pub(crate) fn upsert_timeline(
    conn: &Connection,
    events: &[TimelineEvent],
    source_id: &str,
) -> crate::DbResult<bool> {
    let Some(store) = open_store_for_write(conn)? else {
        return Ok(false);
    };
    require_owner(conn, source_id)?;
    let records = events
        .iter()
        .map(|event| timeline_record(event, source_id))
        .collect::<Vec<_>>();
    persistence_fjall::TimelineRecordStore::from_source_store((*store).clone())
        .upsert_batch(&records, PersistMode::SyncAll)
        .map_err(|error| crate::DbError::System(error.to_string()))?;
    Ok(true)
}
