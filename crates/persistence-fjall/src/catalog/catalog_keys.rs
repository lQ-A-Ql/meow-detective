use super::{CatalogEntryType, FileEntryRecord};
use crate::{
    codec::{encode_key, KeyPart, VersionedEnvelope},
    database::WriteAction,
    errors::{FjallStorageError, Result},
    keyspaces::KeyspaceName,
};

pub(crate) fn decode_record(envelope: VersionedEnvelope) -> Result<FileEntryRecord> {
    if envelope.record_kind != super::RECORD_KIND
        || envelope.schema_version != super::SCHEMA_VERSION
    {
        return Err(FjallStorageError::CatalogRecordIdentityMismatch);
    }
    let record: FileEntryRecord = envelope.decode_payload()?;
    if record.id != envelope.record_id || record.data_source_id != envelope.source_id {
        return Err(FjallStorageError::CatalogRecordIdentityMismatch);
    }
    Ok(record)
}

pub(crate) fn record_key(source_id: &str, record_id: &str) -> Result<Vec<u8>> {
    encode_key(
        super::RECORD_FAMILY,
        &[
            KeyPart::Text(source_id.to_owned()),
            KeyPart::Text(record_id.to_owned()),
        ],
    )
}

pub(crate) fn parent_key(record: &FileEntryRecord) -> Result<Vec<u8>> {
    encode_key(
        super::PARENT_FAMILY,
        &[
            KeyPart::Text(record.data_source_id.clone()),
            KeyPart::Text(
                record
                    .parent_id
                    .as_deref()
                    .unwrap_or(super::ROOT_PARENT)
                    .to_owned(),
            ),
            KeyPart::Text(record.name.clone()),
            KeyPart::Text(record.id.clone()),
        ],
    )
}

pub(crate) fn directory_key(record: &FileEntryRecord) -> Result<Vec<u8>> {
    encode_key(
        super::DIRECTORY_FAMILY,
        &[
            KeyPart::Text(record.data_source_id.clone()),
            KeyPart::Text(
                record
                    .parent_id
                    .as_deref()
                    .unwrap_or(super::ROOT_PARENT)
                    .to_owned(),
            ),
            KeyPart::Text(record.name.clone()),
            KeyPart::Text(record.id.clone()),
        ],
    )
}

pub(crate) fn root_directory_key(record: &FileEntryRecord) -> Result<Vec<u8>> {
    if record.entry_type != CatalogEntryType::Directory || record.parent_id.is_some() {
        return Err(FjallStorageError::InvalidEnvelope(
            "root directory index requires a root directory".into(),
        ));
    }
    encode_key(
        super::ROOT_DIRECTORY_FAMILY,
        &[
            KeyPart::Text(record.data_source_id.clone()),
            KeyPart::Text(record.name.clone()),
            KeyPart::Text(record.id.clone()),
        ],
    )
}

pub(crate) fn path_key(record: &FileEntryRecord) -> Result<Vec<u8>> {
    encode_key(
        super::PATH_FAMILY,
        &[
            KeyPart::Text(record.data_source_id.clone()),
            KeyPart::Text(record.path.clone()),
            KeyPart::Text(record.id.clone()),
        ],
    )
}

pub(crate) fn orphan_key(record: &FileEntryRecord) -> Result<Vec<u8>> {
    encode_key(
        super::ORPHAN_FAMILY,
        &[
            KeyPart::Text(record.data_source_id.clone()),
            KeyPart::Text(record.parent_id.clone().unwrap_or_default()),
            KeyPart::Text(record.name.clone()),
            KeyPart::Text(record.id.clone()),
        ],
    )
}

pub(crate) fn hash_key(record: &FileEntryRecord) -> Result<Vec<u8>> {
    encode_key(
        super::HASH_FAMILY,
        &[
            KeyPart::Text(record.data_source_id.clone()),
            KeyPart::Text(record.hash_sha256.clone().unwrap_or_default()),
            KeyPart::Text(record.id.clone()),
        ],
    )
}

pub(crate) fn tombstone_key(source_id: &str, record_id: &str) -> Result<Vec<u8>> {
    encode_key(
        super::TOMBSTONE_FAMILY,
        &[
            KeyPart::Text(source_id.to_owned()),
            KeyPart::Text(record_id.to_owned()),
        ],
    )
}

pub(crate) fn index_write(key: Vec<u8>, record_key: Vec<u8>) -> WriteAction {
    WriteAction::Put {
        keyspace: KeyspaceName::CatalogIndexes,
        key,
        value: record_key,
    }
}
