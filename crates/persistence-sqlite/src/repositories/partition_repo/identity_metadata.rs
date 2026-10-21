//! Source-local storage for observed disk, partition, and filesystem identities.
use crate::{connection::DbResult, DbError};
use rusqlite::{params, Connection};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PartitionIdentityMetadata {
    pub gpt_partition_uuid: Option<String>,
    pub filesystem_uuid: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SourceIdentityMetadata {
    pub gpt_disk_guid: Option<String>,
    pub partitions: BTreeMap<u32, PartitionIdentityMetadata>,
}

fn prefix(source: &str) -> String {
    format!("identity.{}.", hex::encode(source.as_bytes()))
}

fn retired_prefix(source: &str) -> String {
    format!("partition.uuid.{}.", hex::encode(source.as_bytes()))
}

fn has_metadata_store(conn: &Connection) -> DbResult<bool> {
    conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='source_meta')",
        [],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

fn valid_guid(value: String) -> Option<String> {
    uuid::Uuid::parse_str(&value)
        .ok()
        .filter(|guid| !guid.is_nil())
        .map(|_| value)
}

pub(super) fn read(conn: &Connection, source: &str) -> DbResult<SourceIdentityMetadata> {
    let mut metadata = SourceIdentityMetadata::default();
    if !has_metadata_store(conn)? {
        return Ok(metadata);
    }
    let prefix = prefix(source);
    let mut query = conn.prepare("SELECT key, value FROM source_meta WHERE key GLOB ?1")?;
    let mut rows = query.query([format!("{prefix}*")])?;
    while let Some(row) = rows.next()? {
        let key: String = row.get(0)?;
        let value: String = row.get(1)?;
        let suffix = key.strip_prefix(&prefix).unwrap_or_default();
        if suffix == "gpt-disk-guid" {
            metadata.gpt_disk_guid = valid_guid(value);
            continue;
        }
        let Some(rest) = suffix.strip_prefix("partition.") else {
            continue;
        };
        let Some((index, kind)) = rest.split_once('.') else {
            continue;
        };
        let Ok(index) = index.parse::<u32>() else {
            continue;
        };
        match kind {
            "gpt-uuid" => {
                metadata
                    .partitions
                    .entry(index)
                    .or_default()
                    .gpt_partition_uuid = valid_guid(value);
            }
            "filesystem-uuid" if !value.trim().is_empty() => {
                metadata
                    .partitions
                    .entry(index)
                    .or_default()
                    .filesystem_uuid = Some(value);
            }
            _ => {}
        }
    }
    Ok(metadata)
}

pub(super) fn replace(
    conn: &Connection,
    source: &str,
    metadata: &SourceIdentityMetadata,
) -> DbResult<()> {
    if !has_metadata_store(conn)? {
        if metadata.gpt_disk_guid.is_none() && metadata.partitions.is_empty() {
            return Ok(());
        }
        return Err(DbError::System(
            "source identities require source-local metadata".into(),
        ));
    }
    let prefix = prefix(source);
    conn.execute(
        "DELETE FROM source_meta WHERE key GLOB ?1",
        [format!("{prefix}*")],
    )?;
    // The previous contract had no typed identity namespace. Remove its
    // source-local records during replacement so stale UUIDs cannot survive a
    // no-compatibility import or be mistaken for current observations.
    conn.execute(
        "DELETE FROM source_meta WHERE key GLOB ?1",
        [format!("{}*", retired_prefix(source))],
    )?;
    let mut insert = conn.prepare_cached("INSERT INTO source_meta (key,value) VALUES (?1,?2)")?;
    if let Some(guid) = &metadata.gpt_disk_guid {
        if valid_guid(guid.clone()).is_none() {
            return Err(DbError::System("Invalid GPT disk GUID".into()));
        }
        insert.execute(params![format!("{prefix}gpt-disk-guid"), guid])?;
    }
    for (index, identity) in &metadata.partitions {
        if let Some(guid) = &identity.gpt_partition_uuid {
            if valid_guid(guid.clone()).is_none() {
                return Err(DbError::System("Invalid GPT partition UUID".into()));
            }
            insert.execute(params![format!("{prefix}partition.{index}.gpt-uuid"), guid])?;
        }
        if let Some(uuid) = &identity.filesystem_uuid {
            if uuid.trim().is_empty() {
                return Err(DbError::System("Filesystem UUID must not be empty".into()));
            }
            insert.execute(params![
                format!("{prefix}partition.{index}.filesystem-uuid"),
                uuid
            ])?;
        }
    }
    Ok(())
}
