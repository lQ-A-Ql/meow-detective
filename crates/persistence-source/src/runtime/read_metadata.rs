use super::{ReadOnlySourceStorageHandle, StorageRuntimeError};
use chrono::{DateTime, Utc};
use persistence_fjall::{
    ArtifactRecordStore, CatalogPage, CatalogRecordStore, FacetSummary, SourceGenerationManifest,
    SourceReadSnapshot, TimelineRecordStore,
};
use std::ops::RangeInclusive;

impl ReadOnlySourceStorageHandle {
    /// Read-only handles do not expose the mutable source store.
    ///
    /// ```compile_fail
    /// fn writable(handle: &persistence_source::runtime::ReadOnlySourceStorageHandle) {
    ///     let _ = handle.source_store();
    /// }
    /// ```
    pub fn generation_manifest(
        &self,
        source: &str,
    ) -> Result<SourceGenerationManifest, StorageRuntimeError> {
        persistence_fjall::read_source_generation_manifest(self.store(), source).map_err(Into::into)
    }

    pub fn timeline_facet_summary(
        &self,
        snapshot: &SourceReadSnapshot,
        source: &str,
        kind: Option<&str>,
        start: Option<DateTime<Utc>>,
        end: Option<DateTime<Utc>>,
    ) -> Result<FacetSummary, StorageRuntimeError> {
        TimelineRecordStore::from_source_store(self.store().clone())
            .facet_summary(snapshot, source, kind, start, end)
            .map_err(Into::into)
    }

    pub fn timeline_count_windows(
        &self,
        snapshot: &SourceReadSnapshot,
        source: &str,
        kind: Option<&str>,
        windows: &[RangeInclusive<DateTime<Utc>>],
    ) -> Result<Vec<u64>, StorageRuntimeError> {
        TimelineRecordStore::from_source_store(self.store().clone())
            .count_windows(snapshot, source, kind, windows)
            .map_err(Into::into)
    }

    pub fn catalog_file_count(
        &self,
        snapshot: &SourceReadSnapshot,
        source: &str,
    ) -> Result<u64, StorageRuntimeError> {
        CatalogRecordStore::from_source_store(self.store().clone())
            .file_count(snapshot, source)
            .map_err(Into::into)
    }

    pub fn catalog_cursor_revision(
        &self,
        snapshot: &SourceReadSnapshot,
    ) -> Result<u64, StorageRuntimeError> {
        CatalogRecordStore::from_source_store(self.store().clone())
            .cursor_revision(snapshot)
            .map_err(Into::into)
    }

    pub fn artifact_cursor_revision(
        &self,
        snapshot: &SourceReadSnapshot,
    ) -> Result<u64, StorageRuntimeError> {
        ArtifactRecordStore::from_source_store(self.store().clone())
            .cursor_revision(snapshot)
            .map_err(Into::into)
    }

    pub fn catalog_files_by_path(
        &self,
        snapshot: &SourceReadSnapshot,
        source: &str,
        after: Option<(&str, &str)>,
        limit: usize,
    ) -> Result<CatalogPage, StorageRuntimeError> {
        CatalogRecordStore::from_source_store(self.store().clone())
            .list_files_by_path(snapshot, source, after, limit)
            .map_err(Into::into)
    }
}
