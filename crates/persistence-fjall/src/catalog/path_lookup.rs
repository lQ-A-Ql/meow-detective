use super::{CatalogPage, CatalogRecordStore, FileEntryRecord};
use crate::{
    codec::{encode_key, KeyPart},
    Result, ScanDirection, SourceReadSnapshot,
};

fn normalized(path: &str, partition: u32) -> String {
    let prefix = format!("[P{partition}]/");
    path.trim_start_matches('/')
        .strip_prefix(&prefix)
        .unwrap_or(path.trim_start_matches('/'))
        .to_ascii_lowercase()
}

fn prefix(source: &str, partition: u32, path: &str) -> Result<Vec<u8>> {
    encode_key(
        "catalog.partition-path-ci",
        &[
            KeyPart::Text(source.into()),
            KeyPart::Text(partition.to_string()),
            KeyPart::Text(normalized(path, partition)),
        ],
    )
}

pub(super) fn key(record: &FileEntryRecord) -> Result<Vec<u8>> {
    let mut key = prefix(
        &record.data_source_id,
        record.partition_index.unwrap_or(u32::MAX),
        &record.path,
    )?;
    key.extend(encode_key("", &[KeyPart::Text(record.id.clone())])?);
    Ok(key)
}

impl CatalogRecordStore {
    pub fn find_by_partition_path_ci(
        &self,
        snapshot: &SourceReadSnapshot,
        source: &str,
        partition: u32,
        path: &str,
        cursor: Option<&[u8]>,
        limit: usize,
    ) -> Result<CatalogPage> {
        self.page_from_index(
            snapshot,
            &prefix(source, partition, path)?,
            cursor,
            ScanDirection::Forward,
            limit,
        )
    }
}
