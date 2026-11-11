use super::mapping::{artifact_record, file_record, timeline_record};
use super::open::require_owner;
use super::open_store_for_write;
use domain::{Artifact, FileEntry, TimelineEvent};
use persistence_fjall::{CatalogRecordStore, PersistMode};
use persistence_sqlite::{DbError, DbResult};
use rusqlite::Connection;

pub fn set_partition_index(conn: &Connection, record_id: &str, partition: usize) -> DbResult<bool> {
    let Some(store) = open_store_for_write(conn)? else {
        return Ok(false);
    };
    let repository = CatalogRecordStore::from_source_store((*store).clone());
    let snapshot = repository.read_snapshot();
    let owner = super::source_id(conn)?
        .ok_or_else(|| DbError::System("source identity is missing".into()))?;
    let Some(mut record) = repository
        .get(&snapshot, &owner, record_id)
        .map_err(|error| DbError::System(error.to_string()))?
    else {
        return Ok(false);
    };
    record.partition_index = Some(
        u32::try_from(partition)
            .map_err(|_| DbError::System("partition index exceeds Fjall range".into()))?,
    );
    repository
        .upsert(&record, persistence_fjall::PersistMode::SyncAll)
        .map_err(|error| DbError::System(error.to_string()))?;
    Ok(true)
}

pub fn assign_partition_index_to_subtree(
    conn: &Connection,
    root_id: &str,
    partition: usize,
) -> DbResult<Option<usize>> {
    let Some(store) = open_store_for_write(conn)? else {
        return Ok(None);
    };
    let repository = CatalogRecordStore::from_source_store((*store).clone());
    let owner = super::source_id(conn)?
        .ok_or_else(|| DbError::System("source identity is missing".into()))?;
    let partition = u32::try_from(partition)
        .map_err(|_| DbError::System("partition index exceeds Fjall range".into()))?;
    repository
        .assign_partition_to_subtree(
            &owner,
            root_id,
            partition,
            persistence_fjall::PersistMode::SyncAll,
        )
        .map(Some)
        .map_err(|error| DbError::System(error.to_string()))
}

pub fn update_catalog_path(
    conn: &Connection,
    record_id: &str,
    path: &str,
    source_id: &str,
) -> DbResult<bool> {
    let Some(store) = open_store_for_write(conn)? else {
        return Ok(false);
    };
    let repository = CatalogRecordStore::from_source_store((*store).clone());
    let snapshot = repository.read_snapshot();
    let Some(mut record) = repository
        .get(&snapshot, source_id, record_id)
        .map_err(|error| DbError::System(error.to_string()))?
    else {
        return Ok(false);
    };
    record.path = path.to_owned();
    repository
        .upsert(&record, persistence_fjall::PersistMode::SyncAll)
        .map_err(|error| DbError::System(error.to_string()))?;
    Ok(true)
}

pub fn update_catalog_parent(
    conn: &Connection,
    record_id: &str,
    parent_id: Option<String>,
    source_id: &str,
) -> DbResult<bool> {
    let Some(store) = open_store_for_write(conn)? else {
        return Ok(false);
    };
    let repository = CatalogRecordStore::from_source_store((*store).clone());
    let snapshot = repository.read_snapshot();
    let Some(mut record) = repository
        .get(&snapshot, source_id, record_id)
        .map_err(|error| DbError::System(error.to_string()))?
    else {
        return Ok(false);
    };
    record.parent_id = parent_id;
    repository
        .upsert(&record, persistence_fjall::PersistMode::SyncAll)
        .map_err(|error| DbError::System(error.to_string()))?;
    Ok(true)
}

pub fn upsert_files(conn: &Connection, entries: &[FileEntry]) -> DbResult<bool> {
    let Some(store) = open_store_for_write(conn)? else {
        return Ok(false);
    };
    for entry in entries {
        require_owner(conn, &entry.data_source_id.0)?;
    }
    let records = entries.iter().map(file_record).collect::<Vec<_>>();
    CatalogRecordStore::from_source_store((*store).clone())
        .upsert_batch(&records, PersistMode::SyncAll)
        .map_err(|error| DbError::System(error.to_string()))?;
    Ok(true)
}

pub fn upsert_files_with_partition(
    conn: &Connection,
    entries: &[FileEntry],
    partition_index: usize,
) -> DbResult<bool> {
    let Some(store) = open_store_for_write(conn)? else {
        return Ok(false);
    };
    for entry in entries {
        require_owner(conn, &entry.data_source_id.0)?;
    }
    let partition_index = u32::try_from(partition_index)
        .map_err(|_| DbError::System("partition index exceeds Fjall range".into()))?;
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
        .map_err(|error| DbError::System(error.to_string()))?;
    Ok(true)
}

pub fn upsert_artifacts(
    conn: &Connection,
    artifacts: &[Artifact],
    source_id: &str,
) -> DbResult<bool> {
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
        .map_err(|error| DbError::System(error.to_string()))?;
    Ok(true)
}

pub fn upsert_timeline(
    conn: &Connection,
    events: &[TimelineEvent],
    source_id: &str,
) -> DbResult<bool> {
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
        .map_err(|error| DbError::System(error.to_string()))?;
    Ok(true)
}
