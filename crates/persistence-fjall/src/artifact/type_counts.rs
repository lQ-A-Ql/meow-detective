use super::{ArtifactRecordStore, RECORD_FAMILY};
use crate::{
    codec::{decode_key, encode_key, KeyPart},
    database::{SourceReadSnapshot, WriteAction},
    errors::{FjallStorageError, Result},
    keyspaces::KeyspaceName,
    scan::ScanDirection,
};
use std::collections::BTreeMap;

const TYPE_COUNT_FAMILY: &str = "artifact.type-count";
const TYPE_COUNT_SCHEMA: &str = "artifact.type-count-schema";
const TYPE_COUNT_SCHEMA_VALUE: &[u8] = b"v1";

pub(super) type TypeCountDeltas = BTreeMap<Vec<u8>, i64>;

/// Whether a snapshot-holding mutation may maintain per-type counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CountPolicy {
    /// The ready marker exists: adjust counts in the same write batch.
    Maintain,
    /// The source has no records yet, so counts can start at zero and the ready
    /// marker can be written together with this mutation.
    Bootstrap,
    /// Counts are missing for a non-empty source; leave them absent and rely on
    /// `rebuild_metrics_for_source_with_control` to recreate them.
    Skip,
}

impl CountPolicy {
    pub(super) fn maintains(self) -> bool {
        matches!(self, Self::Maintain | Self::Bootstrap)
    }
}

impl ArtifactRecordStore {
    /// Per-type counts for one source, sorted by artifact type.
    ///
    /// A nonempty source must have a valid count marker before it is queried.
    pub fn type_counts(
        &self,
        snapshot: &SourceReadSnapshot,
        source_id: &str,
    ) -> Result<Vec<(String, u64)>> {
        if !require_counts_or_empty(snapshot, source_id)? {
            return Ok(Vec::new());
        }
        let prefix = count_prefix(source_id)?;
        let entries = snapshot.scan_prefix(
            KeyspaceName::ArtifactRecords,
            &prefix,
            ScanDirection::Forward,
        )?;
        // Encoded keys order by length before bytes; the ordered map restores
        // artifact-type ordering for callers.
        let mut counts = BTreeMap::new();
        for entry in entries {
            counts.insert(count_type(&entry.key)?, count_value(&entry.value)?);
        }
        Ok(counts.into_iter().collect())
    }

    /// Whether the per-source type-count family is authoritative.
    pub fn type_counts_ready(
        &self,
        snapshot: &SourceReadSnapshot,
        source_id: &str,
    ) -> Result<bool> {
        counts_ready(snapshot, source_id)
    }
}

pub(super) fn count_by_type(
    snapshot: &SourceReadSnapshot,
    source_id: &str,
    artifact_type: &str,
) -> Result<u64> {
    if !require_counts_or_empty(snapshot, source_id)? {
        return Ok(0);
    }
    read_count(snapshot, source_id, artifact_type)
}

pub(super) fn counts_ready(snapshot: &SourceReadSnapshot, source_id: &str) -> Result<bool> {
    let Some(value) = snapshot.get(KeyspaceName::Meta, &marker_key(source_id)?)? else {
        return Ok(false);
    };
    if value == TYPE_COUNT_SCHEMA_VALUE {
        Ok(true)
    } else {
        Err(FjallStorageError::InvalidEnvelope(
            "artifact type count schema marker is malformed".to_owned(),
        ))
    }
}

pub(super) fn count_policy(snapshot: &SourceReadSnapshot, source_id: &str) -> Result<CountPolicy> {
    if counts_ready(snapshot, source_id)? {
        return Ok(CountPolicy::Maintain);
    }
    let prefix = encode_key(RECORD_FAMILY, &[KeyPart::Text(source_id.to_owned())])?;
    let probe = snapshot.keyset(
        KeyspaceName::ArtifactRecords,
        &prefix,
        None,
        ScanDirection::Forward,
        1,
    )?;
    if !probe.is_empty() {
        return Ok(CountPolicy::Skip);
    }
    // Invalidated counts may outlive the last record and must not be bootstrapped again.
    let stale_counts = snapshot.keyset(
        KeyspaceName::ArtifactRecords,
        &count_prefix(source_id)?,
        None,
        ScanDirection::Forward,
        1,
    )?;
    Ok(if stale_counts.is_empty() {
        CountPolicy::Bootstrap
    } else {
        CountPolicy::Skip
    })
}

pub(super) fn marker_action(source_id: &str) -> Result<WriteAction> {
    Ok(WriteAction::Put {
        keyspace: KeyspaceName::Meta,
        key: marker_key(source_id)?,
        value: TYPE_COUNT_SCHEMA_VALUE.to_vec(),
    })
}

pub(super) fn adjust(
    deltas: &mut TypeCountDeltas,
    source_id: &str,
    artifact_type: &str,
    delta: i64,
) -> Result<()> {
    let entry = deltas
        .entry(count_key(source_id, artifact_type)?)
        .or_insert(0);
    *entry = entry.checked_add(delta).ok_or_else(|| {
        FjallStorageError::InvalidEnvelope("artifact type count delta overflowed".to_owned())
    })?;
    Ok(())
}

/// Turns accumulated deltas into writes against the pre-batch snapshot.
///
/// A delta that would push a count below zero is an invariant violation, never
/// a silent clamp: it means the count family drifted from the record family.
pub(super) fn append_actions(
    snapshot: &SourceReadSnapshot,
    actions: &mut Vec<WriteAction>,
    deltas: TypeCountDeltas,
) -> Result<()> {
    for (key, delta) in deltas {
        if delta == 0 {
            continue;
        }
        let current = match snapshot.get(KeyspaceName::ArtifactRecords, &key)? {
            Some(value) => count_value(&value)?,
            None => 0,
        };
        let next = if delta > 0 {
            current.checked_add(delta.unsigned_abs())
        } else {
            current.checked_sub(delta.unsigned_abs())
        }
        .ok_or_else(|| {
            FjallStorageError::InvalidEnvelope("artifact type count index underflowed".to_owned())
        })?;
        actions.push(if next == 0 {
            WriteAction::Delete {
                keyspace: KeyspaceName::ArtifactRecords,
                key,
            }
        } else {
            WriteAction::Put {
                keyspace: KeyspaceName::ArtifactRecords,
                key,
                value: next.to_be_bytes().to_vec(),
            }
        });
    }
    Ok(())
}

/// Rewrites the count family of one source from a fresh tally and prunes types
/// that disappeared, so rebuilds cannot leave stale keys behind.
pub(super) fn append_rebuild_actions(
    snapshot: &SourceReadSnapshot,
    actions: &mut Vec<WriteAction>,
    source_id: &str,
    tally: &BTreeMap<String, u64>,
) -> Result<()> {
    let prefix = count_prefix(source_id)?;
    let entries = snapshot.scan_prefix(
        KeyspaceName::ArtifactRecords,
        &prefix,
        ScanDirection::Forward,
    )?;
    for entry in entries {
        if !tally.contains_key(&count_type(&entry.key)?) {
            actions.push(WriteAction::Delete {
                keyspace: KeyspaceName::ArtifactRecords,
                key: entry.key,
            });
        }
    }
    for (artifact_type, count) in tally {
        actions.push(WriteAction::Put {
            keyspace: KeyspaceName::ArtifactRecords,
            key: count_key(source_id, artifact_type)?,
            value: count.to_be_bytes().to_vec(),
        });
    }
    actions.push(marker_action(source_id)?);
    Ok(())
}

fn read_count(snapshot: &SourceReadSnapshot, source_id: &str, artifact_type: &str) -> Result<u64> {
    let Some(value) = snapshot.get(
        KeyspaceName::ArtifactRecords,
        &count_key(source_id, artifact_type)?,
    )?
    else {
        return Ok(0);
    };
    count_value(&value)
}

fn require_counts_or_empty(snapshot: &SourceReadSnapshot, source_id: &str) -> Result<bool> {
    if counts_ready(snapshot, source_id)? {
        return Ok(true);
    }
    let prefix = encode_key(RECORD_FAMILY, &[KeyPart::Text(source_id.to_owned())])?;
    let first = snapshot.keyset(
        KeyspaceName::ArtifactRecords,
        &prefix,
        None,
        ScanDirection::Forward,
        1,
    )?;
    if first.is_empty() {
        return Ok(false);
    }
    Err(FjallStorageError::ArtifactIndexCorrupt)
}

fn count_prefix(source_id: &str) -> Result<Vec<u8>> {
    encode_key(TYPE_COUNT_FAMILY, &[KeyPart::Text(source_id.to_owned())])
}

fn count_key(source_id: &str, artifact_type: &str) -> Result<Vec<u8>> {
    encode_key(
        TYPE_COUNT_FAMILY,
        &[
            KeyPart::Text(source_id.to_owned()),
            KeyPart::Text(artifact_type.to_owned()),
        ],
    )
}

pub(super) fn marker_key(source_id: &str) -> Result<Vec<u8>> {
    encode_key(TYPE_COUNT_SCHEMA, &[KeyPart::Text(source_id.to_owned())])
}

fn count_type(key: &[u8]) -> Result<String> {
    let (family, parts) = decode_key(key)?;
    if family != TYPE_COUNT_FAMILY {
        return Err(FjallStorageError::ArtifactIndexCorrupt);
    }
    match parts.as_slice() {
        [KeyPart::Bytes(_), KeyPart::Bytes(artifact_type)] => {
            String::from_utf8(artifact_type.clone())
                .map_err(|_| FjallStorageError::ArtifactIndexCorrupt)
        }
        _ => Err(FjallStorageError::ArtifactIndexCorrupt),
    }
}

fn count_value(value: &[u8]) -> Result<u64> {
    <[u8; 8]>::try_from(value)
        .map(u64::from_be_bytes)
        .map_err(|_| FjallStorageError::ArtifactIndexCorrupt)
}
