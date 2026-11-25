use super::{browse_keys::bucket, FileEntryRecord};
use crate::{
    encode_key, FjallStorageError, KeyPart, KeyspaceName, Result, SourceReadSnapshot, WriteAction,
};
use std::collections::BTreeMap;

pub(super) type Deltas = BTreeMap<Vec<u8>, i64>;
pub(super) const FAMILY: &str = "catalog.browse-count";

pub(super) fn marker(source: &str) -> Result<Vec<u8>> {
    encode_key("catalog.browse-schema", &[KeyPart::Text(source.into())])
}

pub(super) fn mark(actions: &mut Vec<WriteAction>, source: &str) -> Result<()> {
    actions.push(WriteAction::Put {
        keyspace: KeyspaceName::Meta,
        key: marker(source)?,
        value: vec![1],
    });
    Ok(())
}

pub(super) fn require(snapshot: &SourceReadSnapshot, source: &str) -> Result<()> {
    if snapshot
        .get(KeyspaceName::Meta, &marker(source)?)?
        .as_deref()
        != Some(&[1])
    {
        return Err(FjallStorageError::InvalidEnvelope(
            "catalog browse index is unavailable; import the source again".into(),
        ));
    }
    Ok(())
}

pub(super) fn source_prefix(source: &str) -> Result<Vec<u8>> {
    encode_key(FAMILY, &[KeyPart::Text(source.into())])
}

fn key(source: &str, parent: Option<&str>, bucket: u16) -> Result<Vec<u8>> {
    encode_key(
        FAMILY,
        &[
            KeyPart::Text(source.into()),
            KeyPart::Text(parent.unwrap_or_default().into()),
            KeyPart::U16(bucket),
        ],
    )
}

pub(super) fn adjust(deltas: &mut Deltas, record: &FileEntryRecord, delta: i64) -> Result<()> {
    *deltas
        .entry(key(
            &record.data_source_id,
            record.parent_id.as_deref(),
            bucket(record),
        )?)
        .or_default() += delta;
    Ok(())
}

pub(super) fn stream_adjust(
    writer: &mut crate::database::WriteStream<'_>,
    record: &FileEntryRecord,
    delta: i64,
) -> Result<()> {
    let key = key(
        &record.data_source_id,
        record.parent_id.as_deref(),
        bucket(record),
    )?;
    let previous = match writer.get(KeyspaceName::Meta, &key)? {
        None => 0,
        Some(bytes) => u64::from_be_bytes(bytes.try_into().map_err(|_| {
            FjallStorageError::InvalidEnvelope("catalog browse count is malformed".into())
        })?),
    };
    let count = previous.checked_add_signed(delta).ok_or_else(|| {
        FjallStorageError::InvalidEnvelope("catalog browse count overflow".into())
    })?;
    writer.put(KeyspaceName::Meta, key, count.to_be_bytes().to_vec());
    Ok(())
}

pub(super) fn count(
    snapshot: &SourceReadSnapshot,
    source: &str,
    parent: Option<&str>,
    bucket: u16,
) -> Result<u64> {
    read(snapshot, &key(source, parent, bucket)?)
}

fn read(snapshot: &SourceReadSnapshot, key: &[u8]) -> Result<u64> {
    match snapshot.get(KeyspaceName::Meta, key)? {
        None => Ok(0),
        Some(bytes) => Ok(u64::from_be_bytes(bytes.try_into().map_err(|_| {
            FjallStorageError::InvalidEnvelope("catalog browse count is malformed".into())
        })?)),
    }
}

pub(super) fn append(
    snapshot: &SourceReadSnapshot,
    actions: &mut Vec<WriteAction>,
    deltas: Deltas,
) -> Result<()> {
    for (key, delta) in deltas {
        if delta == 0 {
            continue;
        }
        let value = read(snapshot, &key)?
            .checked_add_signed(delta)
            .ok_or_else(|| {
                FjallStorageError::InvalidEnvelope("catalog browse count overflow".into())
            })?;
        actions.push(WriteAction::Put {
            keyspace: KeyspaceName::Meta,
            key,
            value: value.to_be_bytes().to_vec(),
        });
    }
    Ok(())
}
