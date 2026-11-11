use artifact_storage::{ArtifactStorageError, ArtifactStore, ObjectAddress};
use persistence_fjall::{
    CatalogPage, CatalogRecordStore, DatabaseConfig, FileEntryRecord, FjallStorageError,
    KeyspaceName, PersistMode, ScanDirection, SourceReadSnapshot, SourceStore, TimelinePage,
    TimelineRecord, TimelineRecordStore, VersionedEnvelope, WriteBatchReceipt,
};
use std::{
    marker::PhantomData,
    path::{Path, PathBuf},
    sync::Arc,
};
use thiserror::Error;

use super::open::open_parts;

#[derive(Debug, Error)]
pub enum StorageRuntimeError {
    #[error("invalid source identifier: {0}")]
    InvalidSourceId(String),
    #[error("storage root is not a directory: {0}")]
    InvalidRoot(PathBuf),
    #[error("storage root does not exist: {0}")]
    MissingRoot(PathBuf),
    #[error("Fjall storage operation failed: {0}")]
    Source(#[source] FjallStorageError),
    #[error("artifact storage operation failed: {0}")]
    Artifacts(#[source] ArtifactStorageError),
    #[error("read-only storage requires an existing artifact object directory: {0}")]
    MissingArtifactDirectory(PathBuf),
    #[error("source storage registry lock is poisoned")]
    RegistryPoisoned,
    #[error("source storage is being removed: {0}")]
    SourceClosing(PathBuf),
    #[error("source details are not ready: {0}")]
    SourceNotReady(String),
}

impl From<FjallStorageError> for StorageRuntimeError {
    fn from(error: FjallStorageError) -> Self {
        Self::Source(error)
    }
}

impl From<ArtifactStorageError> for StorageRuntimeError {
    fn from(error: ArtifactStorageError) -> Self {
        Self::Artifacts(error)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReadOnly;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReadWrite;

pub type ReadOnlyStorageRuntime = StorageRuntime<ReadOnly>;
pub type ReadWriteStorageRuntime = StorageRuntime<ReadWrite>;
pub type ReadOnlyCaseStorageHandle = CaseStorageHandle<ReadOnly>;
pub type ReadWriteCaseStorageHandle = CaseStorageHandle<ReadWrite>;
pub type ReadOnlySourceStorageHandle = SourceStorageHandle<ReadOnly>;
pub type ReadWriteSourceStorageHandle = SourceStorageHandle<ReadWrite>;

pub struct StorageRuntime<Mode> {
    source_root: PathBuf,
    source_store: Arc<SourceStore>,
    artifact_store: Arc<ArtifactStore>,
    _mode: PhantomData<Mode>,
}

#[derive(Clone)]
pub struct CaseStorageHandle<Mode> {
    source_root: PathBuf,
    artifact_store: Arc<ArtifactStore>,
    _mode: PhantomData<Mode>,
}

#[derive(Clone)]
pub struct SourceStorageHandle<Mode> {
    source_root: PathBuf,
    source_store: Arc<SourceStore>,
    artifact_store: Arc<ArtifactStore>,
    _mode: PhantomData<Mode>,
}

impl StorageRuntime<ReadOnly> {
    pub fn open(source_root: impl AsRef<Path>) -> Result<Self, StorageRuntimeError> {
        let parts = open_parts(source_root.as_ref(), true, DatabaseConfig::default())?;
        Ok(Self {
            source_root: parts.source_root,
            source_store: parts.source_store,
            artifact_store: parts.artifact_store,
            _mode: PhantomData,
        })
    }

    pub fn case(&self) -> ReadOnlyCaseStorageHandle {
        CaseStorageHandle::new(&self.source_root, Arc::clone(&self.artifact_store))
    }

    pub fn source(&self) -> ReadOnlySourceStorageHandle {
        SourceStorageHandle::new(
            &self.source_root,
            Arc::clone(&self.source_store),
            Arc::clone(&self.artifact_store),
        )
    }

    pub fn source_root(&self) -> &Path {
        &self.source_root
    }
}

impl StorageRuntime<ReadWrite> {
    pub fn open(source_root: impl AsRef<Path>) -> Result<Self, StorageRuntimeError> {
        let parts = open_parts(source_root.as_ref(), false, DatabaseConfig::default())?;
        Ok(Self {
            source_root: parts.source_root,
            source_store: parts.source_store,
            artifact_store: parts.artifact_store,
            _mode: PhantomData,
        })
    }

    pub fn open_with_config(
        source_root: impl AsRef<Path>,
        config: DatabaseConfig,
    ) -> Result<Self, StorageRuntimeError> {
        let parts = open_parts(source_root.as_ref(), false, config)?;
        Ok(Self {
            source_root: parts.source_root,
            source_store: parts.source_store,
            artifact_store: parts.artifact_store,
            _mode: PhantomData,
        })
    }

    pub fn case(&self) -> ReadWriteCaseStorageHandle {
        CaseStorageHandle::new(&self.source_root, Arc::clone(&self.artifact_store))
    }

    pub fn source(&self) -> ReadWriteSourceStorageHandle {
        SourceStorageHandle::new(
            &self.source_root,
            Arc::clone(&self.source_store),
            Arc::clone(&self.artifact_store),
        )
    }

    pub fn source_root(&self) -> &Path {
        &self.source_root
    }
}

impl<Mode> CaseStorageHandle<Mode> {
    fn new(source_root: &Path, artifact_store: Arc<ArtifactStore>) -> Self {
        Self {
            source_root: source_root.to_path_buf(),
            artifact_store,
            _mode: PhantomData,
        }
    }

    pub fn source_root(&self) -> &Path {
        &self.source_root
    }

    pub fn artifacts(&self) -> &ArtifactStore {
        self.artifact_store.as_ref()
    }
}

impl<Mode> SourceStorageHandle<Mode> {
    pub fn source_store(&self) -> &SourceStore {
        self.source_store.as_ref()
    }
    fn new(
        source_root: &Path,
        source_store: Arc<SourceStore>,
        artifact_store: Arc<ArtifactStore>,
    ) -> Self {
        Self {
            source_root: source_root.to_path_buf(),
            source_store,
            artifact_store,
            _mode: PhantomData,
        }
    }

    pub fn source_root(&self) -> &Path {
        &self.source_root
    }

    pub fn artifacts(&self) -> &ArtifactStore {
        self.artifact_store.as_ref()
    }

    pub fn snapshot(&self) -> SourceReadSnapshot {
        self.source_store.database().read_snapshot()
    }

    pub fn read_envelope(
        &self,
        snapshot: &SourceReadSnapshot,
        keyspace: KeyspaceName,
        key: &[u8],
    ) -> Result<Option<VersionedEnvelope>, StorageRuntimeError> {
        self.source_store
            .read_envelope(snapshot, keyspace, key)
            .map_err(StorageRuntimeError::Source)
    }

    pub fn read_artifact(&self, address: &ObjectAddress) -> Result<Vec<u8>, StorageRuntimeError> {
        self.artifact_store
            .read(address)
            .map_err(StorageRuntimeError::Artifacts)
    }

    pub fn catalog_get(
        &self,
        snapshot: &SourceReadSnapshot,
        source_id: &str,
        record_id: &str,
    ) -> Result<Option<FileEntryRecord>, StorageRuntimeError> {
        CatalogRecordStore::from_source_store((*self.source_store).clone())
            .get(snapshot, source_id, record_id)
            .map_err(StorageRuntimeError::Source)
    }

    pub fn catalog_list_by_parent(
        &self,
        snapshot: &SourceReadSnapshot,
        source_id: &str,
        parent_id: Option<&str>,
        cursor: Option<&[u8]>,
        direction: ScanDirection,
        limit: usize,
    ) -> Result<CatalogPage, StorageRuntimeError> {
        CatalogRecordStore::from_source_store((*self.source_store).clone())
            .list_by_parent(snapshot, source_id, parent_id, cursor, direction, limit)
            .map_err(StorageRuntimeError::Source)
    }

    pub fn catalog_list_root_directories(
        &self,
        snapshot: &SourceReadSnapshot,
        source_id: &str,
        cursor: Option<&[u8]>,
        direction: ScanDirection,
        limit: usize,
    ) -> Result<CatalogPage, StorageRuntimeError> {
        CatalogRecordStore::from_source_store((*self.source_store).clone())
            .list_root_directories(snapshot, source_id, cursor, direction, limit)
            .map_err(StorageRuntimeError::Source)
    }

    pub fn catalog_list_all(
        &self,
        snapshot: &SourceReadSnapshot,
        source_id: &str,
        cursor: Option<&[u8]>,
        direction: ScanDirection,
        limit: usize,
    ) -> Result<CatalogPage, StorageRuntimeError> {
        CatalogRecordStore::from_source_store((*self.source_store).clone())
            .list_all(snapshot, source_id, cursor, direction, limit)
            .map_err(StorageRuntimeError::Source)
    }

    pub fn catalog_find_by_path(
        &self,
        snapshot: &SourceReadSnapshot,
        source_id: &str,
        path: &str,
        cursor: Option<&[u8]>,
        direction: ScanDirection,
        limit: usize,
    ) -> Result<CatalogPage, StorageRuntimeError> {
        CatalogRecordStore::from_source_store((*self.source_store).clone())
            .find_by_path(snapshot, source_id, path, cursor, direction, limit)
            .map_err(StorageRuntimeError::Source)
    }

    pub fn catalog_find_by_hash(
        &self,
        snapshot: &SourceReadSnapshot,
        source_id: &str,
        hash_sha256: &str,
        cursor: Option<&[u8]>,
        direction: ScanDirection,
        limit: usize,
    ) -> Result<CatalogPage, StorageRuntimeError> {
        CatalogRecordStore::from_source_store((*self.source_store).clone())
            .find_by_hash(snapshot, source_id, hash_sha256, cursor, direction, limit)
            .map_err(StorageRuntimeError::Source)
    }

    pub fn timeline_list_by_time(
        &self,
        snapshot: &SourceReadSnapshot,
        source_id: &str,
        cursor: Option<&[u8]>,
        direction: ScanDirection,
        limit: usize,
    ) -> Result<TimelinePage, StorageRuntimeError> {
        TimelineRecordStore::from_source_store((*self.source_store).clone())
            .list_by_time(snapshot, source_id, cursor, direction, limit)
            .map_err(StorageRuntimeError::Source)
    }

    pub fn timeline_get(
        &self,
        snapshot: &SourceReadSnapshot,
        source_id: &str,
        event_id: &str,
    ) -> Result<Option<TimelineRecord>, StorageRuntimeError> {
        TimelineRecordStore::from_source_store((*self.source_store).clone())
            .get(snapshot, source_id, event_id)
            .map_err(StorageRuntimeError::Source)
    }

    pub fn timeline_list_by_type(
        &self,
        snapshot: &SourceReadSnapshot,
        source_id: &str,
        event_type: &str,
        cursor: Option<&[u8]>,
        direction: ScanDirection,
        limit: usize,
    ) -> Result<TimelinePage, StorageRuntimeError> {
        TimelineRecordStore::from_source_store((*self.source_store).clone())
            .list_by_type(snapshot, source_id, event_type, cursor, direction, limit)
            .map_err(StorageRuntimeError::Source)
    }

    pub fn timeline_list_by_source_object(
        &self,
        snapshot: &SourceReadSnapshot,
        source_id: &str,
        source_object_id: &str,
        cursor: Option<&[u8]>,
        direction: ScanDirection,
        limit: usize,
    ) -> Result<TimelinePage, StorageRuntimeError> {
        TimelineRecordStore::from_source_store((*self.source_store).clone())
            .list_by_source_object(
                snapshot,
                source_id,
                source_object_id,
                cursor,
                direction,
                limit,
            )
            .map_err(StorageRuntimeError::Source)
    }

    pub fn timeline_cursor_revision(
        &self,
        snapshot: &SourceReadSnapshot,
    ) -> Result<u64, StorageRuntimeError> {
        TimelineRecordStore::from_source_store((*self.source_store).clone())
            .cursor_revision(snapshot)
            .map_err(StorageRuntimeError::Source)
    }
}

impl SourceStorageHandle<ReadWrite> {
    pub fn persist(&self, mode: PersistMode) -> Result<(), StorageRuntimeError> {
        self.source_store
            .database()
            .persist(mode)
            .map_err(StorageRuntimeError::Source)
    }

    pub fn catalog_upsert(
        &self,
        record: &FileEntryRecord,
        mode: PersistMode,
    ) -> Result<(), StorageRuntimeError> {
        CatalogRecordStore::from_source_store((*self.source_store).clone())
            .upsert(record, mode)
            .map_err(StorageRuntimeError::Source)
    }

    pub fn catalog_delete(
        &self,
        source_id: &str,
        record_id: &str,
        mode: PersistMode,
    ) -> Result<bool, StorageRuntimeError> {
        CatalogRecordStore::from_source_store((*self.source_store).clone())
            .delete(source_id, record_id, mode)
            .map_err(StorageRuntimeError::Source)
    }

    pub fn timeline_upsert(
        &self,
        record: &TimelineRecord,
        mode: PersistMode,
    ) -> Result<(), StorageRuntimeError> {
        TimelineRecordStore::from_source_store((*self.source_store).clone())
            .upsert(record, mode)
            .map_err(StorageRuntimeError::Source)
    }

    pub fn timeline_delete(
        &self,
        source_id: &str,
        record_id: &str,
        mode: PersistMode,
    ) -> Result<bool, StorageRuntimeError> {
        TimelineRecordStore::from_source_store((*self.source_store).clone())
            .delete(source_id, record_id, mode)
            .map_err(StorageRuntimeError::Source)
    }

    pub fn write_envelope(
        &self,
        keyspace: KeyspaceName,
        key: Vec<u8>,
        envelope: &VersionedEnvelope,
        mode: PersistMode,
    ) -> Result<WriteBatchReceipt, StorageRuntimeError> {
        self.source_store
            .write_envelope(keyspace, key, envelope, mode)
            .map_err(StorageRuntimeError::Source)
    }
}
