use super::{record_key, CatalogEntryType, FileEntryRecord};
use crate::{encode_key, FjallStorageError, KeyPart, KeyspaceName, Result, WriteAction};
use serde::Serialize;

pub(super) const FAMILY: &str = "catalog.browse";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatalogSort {
    Name,
    Size,
    Modified,
    Extension,
}

impl CatalogSort {
    pub(super) fn code(self) -> u16 {
        match self {
            Self::Name => 0,
            Self::Size => 1,
            Self::Modified => 2,
            Self::Extension => 3,
        }
    }
}

pub(super) fn bucket(record: &FileEntryRecord) -> u16 {
    let kind = u16::from(record.entry_type == CatalogEntryType::File);
    kind * 4 + u16::from(record.deleted) * 2 + u16::from(record.hidden || record.system)
}

pub(super) fn prefix(
    source: &str,
    parent: Option<&str>,
    sort: CatalogSort,
    bucket: u16,
) -> Result<Vec<u8>> {
    encode_key(
        FAMILY,
        &[
            KeyPart::Text(source.into()),
            KeyPart::Text(parent.unwrap_or_default().into()),
            KeyPart::U16(sort.code()),
            KeyPart::U16(bucket),
        ],
    )
}

fn ordered(value: &impl Serialize) -> Result<Vec<u8>> {
    memcomparable::to_vec(value)
        .map_err(|error| FjallStorageError::Serialization(error.to_string()))
}

/// Match the existing natural ordering, including case and leading-zero ties.
pub fn natural_name_key(name: &str) -> Result<Vec<u8>> {
    let mut tokens = Vec::with_capacity(name.len() * 2);
    let mut chars = name.chars().peekable();
    while let Some(character) = chars.next() {
        if character.is_ascii_digit() {
            let mut digits = String::from(character);
            while chars.peek().is_some_and(char::is_ascii_digit) {
                digits.push(chars.next().expect("peeked digit"));
            }
            let significant = digits.trim_start_matches('0');
            tokens.extend_from_slice(b"00");
            tokens.extend_from_slice(&(significant.len() as u32).to_be_bytes());
            tokens.extend_from_slice(significant.as_bytes());
            tokens.extend_from_slice(&(digits.len() as u32).to_be_bytes());
        } else {
            let mut buffer = [0; 4];
            tokens.extend_from_slice(
                character
                    .to_ascii_lowercase()
                    .encode_utf8(&mut buffer)
                    .as_bytes(),
            );
            tokens.extend_from_slice(character.encode_utf8(&mut buffer).as_bytes());
        }
    }
    let mut serializer = memcomparable::Serializer::new(Vec::new());
    serde::Serializer::serialize_bytes(&mut serializer, &tokens)
        .map_err(|error| FjallStorageError::Serialization(error.to_string()))?;
    Ok(serializer.into_inner())
}

pub(super) fn primary(record: &FileEntryRecord, sort: CatalogSort) -> Result<Vec<u8>> {
    match sort {
        CatalogSort::Name => natural_name_key(&record.name),
        CatalogSort::Size => ordered(&record.size.unwrap_or(0)),
        CatalogSort::Modified => ordered(
            &record
                .modified
                .map(|stamp| (stamp.timestamp(), stamp.timestamp_subsec_nanos())),
        ),
        CatalogSort::Extension => natural_name_key(
            &record
                .ext
                .as_deref()
                .unwrap_or_default()
                .to_ascii_lowercase(),
        ),
    }
}

pub(super) fn write_indexes(
    actions: &mut Vec<WriteAction>,
    record: &FileEntryRecord,
    delete: bool,
) -> Result<()> {
    let name = natural_name_key(&record.name)?;
    let id = ordered(&record.id)?;
    let value = record_key(&record.data_source_id, &record.id)?;
    for sort in [
        CatalogSort::Name,
        CatalogSort::Size,
        CatalogSort::Modified,
        CatalogSort::Extension,
    ] {
        let mut key = prefix(
            &record.data_source_id,
            record.parent_id.as_deref(),
            sort,
            bucket(record),
        )?;
        key.extend(primary(record, sort)?);
        if sort != CatalogSort::Name {
            key.extend_from_slice(&name);
        }
        key.extend_from_slice(&id);
        actions.push(if delete {
            WriteAction::Delete {
                keyspace: KeyspaceName::CatalogIndexes,
                key,
            }
        } else {
            WriteAction::Put {
                keyspace: KeyspaceName::CatalogIndexes,
                key,
                value: value.clone(),
            }
        });
    }
    Ok(())
}
