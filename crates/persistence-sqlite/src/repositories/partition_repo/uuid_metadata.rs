//! Optional evidence-derived GPT identities in the existing source metadata
//! store. Older cases retain their schema and simply have no recorded UUIDs.
use crate::{connection::DbResult, DbError};
use rusqlite::{params, Connection};
use std::collections::BTreeMap;

fn namespace(source: &str) -> String {
    // Encoding keeps arbitrary source IDs out of GLOB's pattern syntax and
    // prevents one source's prefix from matching another source's identities.
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

pub(super) fn read(conn: &Connection, source: &str) -> DbResult<BTreeMap<u32, String>> {
    let mut identities = BTreeMap::new();
    if !has_metadata_store(conn)? {
        return Ok(identities);
    }
    let prefix = namespace(source);
    let mut query = conn.prepare("SELECT key, value FROM source_meta WHERE key GLOB ?1")?;
    let mut rows = query.query([format!("{prefix}*")])?;
    while let Some(row) = rows.next()? {
        let key: String = row.get(0)?;
        let value: String = row.get(1)?;
        let index = key
            .strip_prefix(&prefix)
            .and_then(|suffix| suffix.parse().ok());
        let uuid = uuid::Uuid::parse_str(&value)
            .ok()
            .filter(|uuid| !uuid.is_nil());
        if let (Some(index), Some(_)) = (index, uuid) {
            identities.insert(index, value);
        } else {
            tracing::warn!("Ignoring malformed GPT partition UUID metadata");
        }
    }
    Ok(identities)
}

pub(super) fn replace(
    conn: &Connection,
    source: &str,
    identities: &BTreeMap<u32, String>,
) -> DbResult<()> {
    if !has_metadata_store(conn)? {
        if identities.is_empty() {
            return Ok(());
        }
        return Err(DbError::System(
            "GPT partition UUIDs require source-local metadata".into(),
        ));
    }
    conn.execute(
        "DELETE FROM source_meta WHERE key GLOB ?1",
        [format!("{}*", namespace(source))],
    )?;
    let mut insert = conn.prepare_cached("INSERT INTO source_meta (key,value) VALUES (?1,?2)")?;
    for (index, value) in identities {
        let uuid = uuid::Uuid::parse_str(value)
            .map_err(|_| DbError::System("Invalid GPT partition UUID".into()))?;
        if uuid.is_nil() {
            return Err(DbError::System("GPT partition UUID must not be nil".into()));
        }
        insert.execute(params![format!("{}{index}", namespace(source)), value])?;
    }
    Ok(())
}
