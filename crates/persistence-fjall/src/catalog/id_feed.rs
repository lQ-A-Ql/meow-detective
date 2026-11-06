use super::{CatalogPage, CatalogRecordStore};
use crate::{encode_key, FjallStorageError, KeyPart, Result, ScanDirection, SourceReadSnapshot};

const FAMILY: &str = "catalog.id";

pub(super) fn key(source: &str, id: &str) -> Result<Vec<u8>> {
    if id.contains('\0') {
        return Err(FjallStorageError::InvalidEnvelope(
            "catalog id contains NUL".into(),
        ));
    }
    let mut key = encode_key(FAMILY, &[KeyPart::Text(source.into())])?;
    key.extend_from_slice(id.as_bytes());
    Ok(key)
}

impl CatalogRecordStore {
    pub fn list_by_id(
        &self,
        snapshot: &SourceReadSnapshot,
        source: &str,
        after: Option<&str>,
        limit: usize,
    ) -> Result<CatalogPage> {
        let prefix = encode_key(FAMILY, &[KeyPart::Text(source.into())])?;
        let cursor = after.map(|id| key(source, id)).transpose()?;
        self.page_from_index(
            snapshot,
            &prefix,
            cursor.as_deref(),
            ScanDirection::Forward,
            limit,
        )
    }
}
