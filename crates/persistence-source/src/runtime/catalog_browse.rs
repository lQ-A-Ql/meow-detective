use super::{ReadOnlySourceStorageHandle, StorageRuntimeError};
use persistence_fjall::{
    CatalogBrowsePage, CatalogBrowseQuery, CatalogRecordStore, SourceReadSnapshot,
};

impl ReadOnlySourceStorageHandle {
    pub fn catalog_browse(
        &self,
        snapshot: &SourceReadSnapshot,
        query: &CatalogBrowseQuery<'_>,
    ) -> Result<CatalogBrowsePage, StorageRuntimeError> {
        CatalogRecordStore::from_source_store(self.store().clone())
            .browse(snapshot, query)
            .map_err(StorageRuntimeError::Source)
    }
}
