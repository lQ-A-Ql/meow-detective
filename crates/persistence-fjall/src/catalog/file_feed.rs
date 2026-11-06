use super::{CatalogPage, CatalogRecordStore, FileEntryRecord};
use crate::{
    encode_key, FjallStorageError, KeyPart, KeyspaceName, Result, ScanDirection,
    SourceReadSnapshot, VersionedEnvelope,
};

const FAMILY: &str = "catalog.file-path";

fn ordered_key(source: &str, path: &str, id: &str) -> Result<Vec<u8>> {
    if path.contains('\0') || id.contains('\0') {
        return Err(FjallStorageError::InvalidEnvelope(
            "catalog path or id contains NUL".into(),
        ));
    }
    let mut key = encode_key(FAMILY, &[KeyPart::Text(source.into())])?;
    key.extend_from_slice(path.as_bytes());
    key.push(0);
    key.extend_from_slice(id.as_bytes());
    Ok(key)
}

pub(super) fn key(record: &FileEntryRecord) -> Result<Vec<u8>> {
    ordered_key(&record.data_source_id, &record.path, &record.id)
}

impl CatalogRecordStore {
    pub fn list_files_by_path(
        &self,
        snapshot: &SourceReadSnapshot,
        source: &str,
        after: Option<(&str, &str)>,
        limit: usize,
    ) -> Result<CatalogPage> {
        if limit == 0 {
            return Ok(CatalogPage {
                records: Vec::new(),
                next_cursor: None,
            });
        }
        let prefix = encode_key(FAMILY, &[KeyPart::Text(source.into())])?;
        let cursor = after
            .map(|(path, id)| ordered_key(source, path, id))
            .transpose()?;
        let entries = snapshot.keyset(
            KeyspaceName::CatalogIndexes,
            &prefix,
            cursor.as_deref(),
            ScanDirection::Forward,
            limit.saturating_add(1),
        )?;
        let has_more = entries.len() > limit;
        let mut records = Vec::with_capacity(entries.len().min(limit));
        let mut next_cursor = None;
        for entry in entries.into_iter().take(limit) {
            let value = snapshot
                .get(KeyspaceName::CatalogRecords, &entry.value)?
                .ok_or(FjallStorageError::CatalogIndexCorrupt)?;
            let record = super::decode_record(VersionedEnvelope::decode(&value)?)?;
            if key(&record)? != entry.key {
                return Err(FjallStorageError::CatalogIndexCorrupt);
            }
            next_cursor = has_more.then_some(entry.key);
            records.push(record);
        }
        Ok(CatalogPage {
            records,
            next_cursor,
        })
    }
}
