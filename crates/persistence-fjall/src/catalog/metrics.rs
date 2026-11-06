use super::CatalogRecordStore;
use crate::{Result, SourceReadSnapshot};

impl CatalogRecordStore {
    pub fn count(&self, snapshot: &SourceReadSnapshot, source: Option<&str>) -> Result<u64> {
        crate::record_metrics::count(snapshot, "catalog", source)
    }

    pub fn file_count(&self, snapshot: &SourceReadSnapshot, source: &str) -> Result<u64> {
        crate::record_metrics::count(snapshot, "catalog.files", Some(source))
    }

    pub fn cursor_revision(&self, snapshot: &SourceReadSnapshot) -> Result<u64> {
        crate::record_metrics::revision(snapshot, "catalog")
    }
}
