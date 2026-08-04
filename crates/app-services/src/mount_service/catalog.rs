use std::sync::{Arc, Mutex};

use domain::{DataSourceId, EntryType, FileEntry, FileEntryId};
use evidence_core::FileSystemReader;
use evidence_mount::{
    DirectoryPage, MountError, MountFileHandle, MountFileSystem, MountNode, MountPath,
};
use persistence_sqlite::repositories::file_repo::FileRepo;
use rusqlite::Connection;

use super::cache::{MountMetadataCache, DEFAULT_METADATA_CACHE_ENTRIES};
use super::directory_cache::{
    DirectorySnapshot, DirectorySnapshotCache, DEFAULT_DIRECTORY_CACHE_BYTES,
};
use super::handle::{FilesystemRangeHandle, SharedFilesystem};
use crate::file_service::entry_image_path_candidates;

pub(crate) struct CatalogMountFileSystem {
    source_conn: Mutex<Connection>,
    filesystem: SharedFilesystem,
    metadata_cache: Mutex<MountMetadataCache>,
    directory_cache: Mutex<DirectorySnapshotCache>,
    data_source_id: DataSourceId,
    partition_index: usize,
}

impl CatalogMountFileSystem {
    pub(crate) fn new(
        source_conn: Connection,
        filesystem: Box<dyn FileSystemReader + Send>,
        data_source_id: DataSourceId,
        partition_index: usize,
    ) -> Self {
        Self {
            source_conn: Mutex::new(source_conn),
            filesystem: Arc::new(Mutex::new(filesystem)),
            metadata_cache: Mutex::new(MountMetadataCache::new(DEFAULT_METADATA_CACHE_ENTRIES)),
            directory_cache: Mutex::new(DirectorySnapshotCache::new(DEFAULT_DIRECTORY_CACHE_BYTES)),
            data_source_id,
            partition_index,
        }
    }

    fn entry_for_path(&self, path: &MountPath) -> Result<Arc<FileEntry>, MountError> {
        if let Some(entry) = self
            .metadata_cache
            .lock()
            .map_err(|_| MountError::Filesystem("mount metadata cache is poisoned".to_string()))?
            .get(path)
        {
            return Ok(entry);
        }
        if let Some(entry) = self
            .directory_cache
            .lock()
            .map_err(|_| MountError::Filesystem("directory cache is poisoned".to_string()))?
            .find_entry(path)
        {
            self.metadata_cache
                .lock()
                .map_err(|_| {
                    MountError::Filesystem("mount metadata cache is poisoned".to_string())
                })?
                .insert(path.clone(), Arc::clone(&entry));
            return Ok(entry);
        }
        let connection = self
            .source_conn
            .lock()
            .map_err(|_| MountError::Filesystem("source catalog lock is poisoned".to_string()))?;
        let repo = FileRepo::new(&connection);
        let entry = if path.is_root() {
            repo.find_root_for_partition(&self.data_source_id, self.partition_index)
        } else {
            let relative = path.as_str().trim_start_matches('/');
            repo.find_by_partition_and_path(&self.data_source_id, self.partition_index, relative)
        }
        .map_err(|error| MountError::Filesystem(error.to_string()))?;
        let entry = Arc::new(entry.ok_or_else(|| MountError::NotFound(path.to_string()))?);
        self.metadata_cache
            .lock()
            .map_err(|_| MountError::Filesystem("mount metadata cache is poisoned".to_string()))?
            .insert(path.clone(), Arc::clone(&entry));
        Ok(entry)
    }

    fn directory_snapshot(
        &self,
        path: &MountPath,
        parent: &FileEntry,
    ) -> Result<Arc<DirectorySnapshot>, MountError> {
        if let Some(snapshot) = self
            .directory_cache
            .lock()
            .map_err(|_| MountError::Filesystem("directory cache is poisoned".to_string()))?
            .get(path)
        {
            return Ok(snapshot);
        }

        let connection = self
            .source_conn
            .lock()
            .map_err(|_| MountError::Filesystem("source catalog lock is poisoned".to_string()))?;
        if let Some(snapshot) = self
            .directory_cache
            .lock()
            .map_err(|_| MountError::Filesystem("directory cache is poisoned".to_string()))?
            .get(path)
        {
            return Ok(snapshot);
        }
        let entries = FileRepo::new(&connection)
            .find_mount_children_for_partition(
                &FileEntryId(parent.id.0.clone()),
                &self.data_source_id,
                self.partition_index,
            )
            .map_err(|error| MountError::Filesystem(error.to_string()))?;
        let mut skipped_entries = 0usize;
        let mut snapshot_entries = Vec::with_capacity(entries.len());
        for entry in entries {
            let entry = Arc::new(entry);
            let Ok(child_path) = Self::child_path(path, &entry.name) else {
                skipped_entries = skipped_entries.saturating_add(1);
                continue;
            };
            let node = self.node_for_entry(&child_path, &entry);
            snapshot_entries.push((node, entry));
        }
        if skipped_entries > 0 {
            tracing::warn!(
                directory = %path,
                skipped_entries,
                "Skipped directory entries that cannot be represented by the Windows mount"
            );
        }
        let snapshot = Arc::new(DirectorySnapshot::new(snapshot_entries));
        self.directory_cache
            .lock()
            .map_err(|_| MountError::Filesystem("directory cache is poisoned".to_string()))
            .map(|mut cache| cache.insert(path.clone(), snapshot))
    }

    fn node_for_entry(&self, path: &MountPath, entry: &FileEntry) -> MountNode {
        MountNode {
            path: path.clone(),
            name: entry.name.clone(),
            is_dir: entry.entry_type == EntryType::Directory,
            size: entry.size.unwrap_or(0),
            read_only: true,
            hidden: entry.hidden,
            system: entry.system,
            encrypted: entry.encrypted,
            created_at: entry.created_at.map(Into::into),
            modified_at: entry.modified_at.map(Into::into),
            accessed_at: entry.accessed_at.map(Into::into),
            source_file_id: Some(entry.id.0.clone()),
        }
    }

    fn child_path(parent: &MountPath, name: &str) -> Result<MountPath, MountError> {
        if parent.is_root() {
            MountPath::parse(name)
        } else {
            MountPath::parse(&format!("{}/{}", parent.as_str(), name))
        }
    }
}

impl MountFileSystem for CatalogMountFileSystem {
    fn lookup(&self, path: &MountPath) -> Result<MountNode, MountError> {
        let entry = self.entry_for_path(path)?;
        Ok(self.node_for_entry(path, &entry))
    }

    fn read_directory(
        &self,
        path: &MountPath,
        cursor: Option<&str>,
        limit: u32,
    ) -> Result<DirectoryPage, MountError> {
        let parent = self.entry_for_path(path)?;
        if parent.entry_type != EntryType::Directory {
            return Err(MountError::NotDirectory(path.to_string()));
        }
        let offset = cursor
            .unwrap_or("0")
            .parse::<usize>()
            .map_err(|_| MountError::InvalidCursor)?;
        let snapshot = self.directory_snapshot(path, &parent)?;
        let nodes = snapshot.page(offset, limit as usize).to_vec();
        let next_offset = offset.saturating_add(nodes.len());
        let next_cursor = (next_offset < snapshot.len()).then(|| next_offset.to_string());
        Ok(DirectoryPage {
            entries: nodes,
            next_cursor,
        })
    }

    fn open_read(&self, path: &MountPath) -> Result<Box<dyn MountFileHandle>, MountError> {
        let entry = self.entry_for_path(path)?;
        if entry.entry_type == EntryType::Directory {
            return Err(MountError::IsDirectory(path.to_string()));
        }
        if entry.encrypted {
            return Err(MountError::Filesystem(
                "file content is encrypted".to_string(),
            ));
        }
        Ok(Box::new(FilesystemRangeHandle::new(
            Arc::clone(&self.filesystem),
            entry_image_path_candidates(&entry),
            entry.size.unwrap_or(0),
        )))
    }
}
