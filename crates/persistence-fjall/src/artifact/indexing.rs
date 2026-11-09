use super::{record_key, ArtifactRecord, OBJECT_INDEX, TYPE_INDEX};
use crate::{
    codec::{encode_key, KeyPart},
    database::WriteAction,
    errors::{FjallStorageError, Result},
    keyspaces::KeyspaceName,
};
use chrono::{DateTime, Utc};

const CREATED_INDEX: &str = "artifact.created";
const FAMILY_CREATED_INDEX: &str = "artifact.family-created";
const TIMESTAMP_SORT_MASK: u64 = 1 << 63;

pub(super) fn index_writes(record: &ArtifactRecord) -> Result<Vec<WriteAction>> {
    let value = record_key(&record.data_source_id, &record.id)?;
    let mut actions = vec![WriteAction::Put {
        keyspace: KeyspaceName::ArtifactIndexes,
        key: encode_key(
            TYPE_INDEX,
            &[
                KeyPart::Text(record.data_source_id.clone()),
                KeyPart::Text(record.artifact_type.clone()),
                KeyPart::Text(record.id.clone()),
            ],
        )?,
        value: value.clone(),
    }];
    if let Some(created_at) = created_at(record)? {
        actions.push(WriteAction::Put {
            keyspace: KeyspaceName::ArtifactIndexes,
            key: created_index_key(&record.data_source_id, &created_at.to_rfc3339(), &record.id)?,
            value: value.clone(),
        });
        actions.push(WriteAction::Put {
            keyspace: KeyspaceName::ArtifactIndexes,
            key: family_created_index_key(
                &record.data_source_id,
                &record.artifact_type,
                &created_at.to_rfc3339(),
                &record.id,
            )?,
            value: value.clone(),
        });
    }
    if let Some(source_object_id) = &record.source_object_id {
        actions.push(WriteAction::Put {
            keyspace: KeyspaceName::ArtifactIndexes,
            key: encode_key(
                OBJECT_INDEX,
                &[
                    KeyPart::Text(record.data_source_id.clone()),
                    KeyPart::Text(source_object_id.clone()),
                    KeyPart::Text(record.id.clone()),
                ],
            )?,
            value,
        });
    }
    Ok(actions)
}

pub(super) fn index_deletes(record: &ArtifactRecord) -> Result<Vec<WriteAction>> {
    let mut actions = vec![WriteAction::Delete {
        keyspace: KeyspaceName::ArtifactIndexes,
        key: encode_key(
            TYPE_INDEX,
            &[
                KeyPart::Text(record.data_source_id.clone()),
                KeyPart::Text(record.artifact_type.clone()),
                KeyPart::Text(record.id.clone()),
            ],
        )?,
    }];
    if let Some(created_at) = created_at(record)? {
        actions.push(WriteAction::Delete {
            keyspace: KeyspaceName::ArtifactIndexes,
            key: created_index_key(&record.data_source_id, &created_at.to_rfc3339(), &record.id)?,
        });
        actions.push(WriteAction::Delete {
            keyspace: KeyspaceName::ArtifactIndexes,
            key: family_created_index_key(
                &record.data_source_id,
                &record.artifact_type,
                &created_at.to_rfc3339(),
                &record.id,
            )?,
        });
    }
    if let Some(source_object_id) = &record.source_object_id {
        actions.push(WriteAction::Delete {
            keyspace: KeyspaceName::ArtifactIndexes,
            key: encode_key(
                OBJECT_INDEX,
                &[
                    KeyPart::Text(record.data_source_id.clone()),
                    KeyPart::Text(source_object_id.clone()),
                    KeyPart::Text(record.id.clone()),
                ],
            )?,
        });
    }
    Ok(actions)
}

fn created_at(record: &ArtifactRecord) -> Result<Option<DateTime<Utc>>> {
    let Some(value) = record
        .attrs
        .get("createdAt")
        .and_then(serde_json::Value::as_str)
    else {
        return Ok(None);
    };
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|error| FjallStorageError::InvalidEnvelope(error.to_string()))
        .map(Some)
}

fn created_index_key(source_id: &str, created_at: &str, id: &str) -> Result<Vec<u8>> {
    let timestamp = parse_created_at(created_at)?;
    encode_key(
        CREATED_INDEX,
        &[
            KeyPart::Text(source_id.to_owned()),
            KeyPart::InverseU64(timestamp_sort_value(timestamp)),
            KeyPart::Text(id.to_owned()),
        ],
    )
}

fn family_created_index_key(
    source_id: &str,
    family: &str,
    created_at: &str,
    id: &str,
) -> Result<Vec<u8>> {
    let timestamp = parse_created_at(created_at)?;
    encode_key(
        FAMILY_CREATED_INDEX,
        &[
            KeyPart::Text(source_id.to_owned()),
            KeyPart::Text(family.to_owned()),
            KeyPart::InverseU64(timestamp_sort_value(timestamp)),
            KeyPart::Text(id.to_owned()),
        ],
    )
}

fn parse_created_at(value: &str) -> Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|error| FjallStorageError::InvalidEnvelope(error.to_string()))
}

fn timestamp_sort_value(timestamp: DateTime<Utc>) -> u64 {
    (timestamp.timestamp_micros() as u64) ^ TIMESTAMP_SORT_MASK
}
