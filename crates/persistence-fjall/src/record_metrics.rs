use crate::{
    encode_key, FjallStorageError, KeyPart, KeyspaceName, Result, SourceReadSnapshot, WriteAction,
};
use std::collections::BTreeMap;

pub(crate) type CountDeltas = BTreeMap<Vec<u8>, i64>;

fn key(domain: &str, source: Option<&str>) -> Result<Vec<u8>> {
    let parts = source
        .map(|source| vec![KeyPart::Text(source.into())])
        .unwrap_or_default();
    encode_key(&format!("{domain}.count"), &parts)
}

pub(crate) fn count(
    snapshot: &SourceReadSnapshot,
    domain: &str,
    source: Option<&str>,
) -> Result<u64> {
    read(snapshot, &key(domain, source)?)
}

fn read(snapshot: &SourceReadSnapshot, key: &[u8]) -> Result<u64> {
    let Some(value) = snapshot.get(KeyspaceName::Meta, key)? else {
        return Ok(0);
    };
    let bytes = <[u8; 8]>::try_from(value.as_slice())
        .map_err(|_| FjallStorageError::InvalidEnvelope("record metric is malformed".into()))?;
    Ok(u64::from_be_bytes(bytes))
}

pub(crate) fn adjust(
    deltas: &mut CountDeltas,
    domain: &str,
    source: &str,
    delta: i64,
) -> Result<()> {
    for source in [None, Some(source)] {
        *deltas.entry(key(domain, source)?).or_default() += delta;
    }
    Ok(())
}

pub(crate) fn append(
    snapshot: &SourceReadSnapshot,
    actions: &mut Vec<WriteAction>,
    deltas: CountDeltas,
) -> Result<()> {
    for (key, delta) in deltas {
        if delta == 0 {
            continue;
        }
        let value = read(snapshot, &key)?
            .checked_add_signed(delta)
            .ok_or_else(|| FjallStorageError::InvalidEnvelope("record count overflowed".into()))?;
        actions.push(WriteAction::Put {
            keyspace: KeyspaceName::Meta,
            key,
            value: value.to_be_bytes().to_vec(),
        });
    }
    Ok(())
}

pub(crate) fn set(
    actions: &mut Vec<WriteAction>,
    domain: &str,
    source: Option<&str>,
    value: u64,
) -> Result<()> {
    actions.push(WriteAction::Put {
        keyspace: KeyspaceName::Meta,
        key: key(domain, source)?,
        value: value.to_be_bytes().to_vec(),
    });
    Ok(())
}

pub(crate) fn revision(snapshot: &SourceReadSnapshot, domain: &str) -> Result<u64> {
    read(snapshot, &encode_key(&format!("{domain}.revision"), &[])?)
}

pub(crate) fn advance(
    snapshot: &SourceReadSnapshot,
    actions: &mut Vec<WriteAction>,
    domain: &str,
) -> Result<()> {
    let value = revision(snapshot, domain)?
        .checked_add(1)
        .ok_or_else(|| FjallStorageError::InvalidEnvelope("record revision overflowed".into()))?;
    actions.push(WriteAction::Put {
        keyspace: KeyspaceName::Meta,
        key: encode_key(&format!("{domain}.revision"), &[])?,
        value: value.to_be_bytes().to_vec(),
    });
    Ok(())
}
