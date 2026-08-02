use std::{collections::HashMap, sync::Arc};

use evidence_core::FileSystemReader;

use crate::{
    ceph_reconstruction::DerivedRbdRuntime,
    file_service::{
        derived_filesystem::open_derived_filesystem,
        filesystem_locators::{
            derived_filesystem_locator_scope, persist_filesystem_locators,
            restore_filesystem_locators, RestoredFilesystemLocatorCounts,
        },
        viewer::{
            descriptor_image_path_candidates, exact_partition_candidate, PreviewDescriptor,
            PreviewPartitionCandidate,
        },
        FileServiceError,
    },
};

const MAX_RESOLVED_PATH_CACHE_ENTRIES: usize = 4_096;

#[derive(Debug, Clone)]
struct ResolvedFilePath {
    filesystem_key: String,
    path: String,
}

#[derive(Default)]
pub(super) struct DerivedSourceReadCache {
    filesystems: HashMap<String, Box<dyn FileSystemReader + Send>>,
    filesystem_candidates: HashMap<String, PreviewPartitionCandidate>,
    filesystem_locator_scopes: HashMap<String, String>,
    persisted_locator_counts: HashMap<String, RestoredFilesystemLocatorCounts>,
    resolved_paths: HashMap<String, ResolvedFilePath>,
    filesystem_open_operations: u64,
}

impl DerivedSourceReadCache {
    pub(super) fn read_file_header(
        &mut self,
        source_conn: &rusqlite::Connection,
        data_source_id: &domain::DataSourceId,
        runtime: &Arc<DerivedRbdRuntime>,
        descriptor: &PreviewDescriptor,
        max_bytes: usize,
    ) -> Result<Vec<u8>, FileServiceError> {
        let bounded = max_bytes.min(usize::try_from(descriptor.size).unwrap_or(usize::MAX));
        let mut bytes =
            Vec::with_capacity(bounded.min(infrastructure::constants::MAX_RANGE_LENGTH));
        let mut offset = 0u64;
        while bytes.len() < bounded {
            let length = (bounded - bytes.len()).min(infrastructure::constants::MAX_RANGE_LENGTH);
            let chunk = self.read_file_range(
                source_conn,
                data_source_id,
                runtime,
                descriptor,
                offset,
                length,
            )?;
            if chunk.is_empty() {
                break;
            }
            let short_read = chunk.len() < length;
            offset = offset.saturating_add(chunk.len() as u64);
            bytes.extend_from_slice(&chunk);
            if short_read {
                break;
            }
        }
        Ok(bytes)
    }

    fn read_file_range(
        &mut self,
        source_conn: &rusqlite::Connection,
        data_source_id: &domain::DataSourceId,
        runtime: &Arc<DerivedRbdRuntime>,
        descriptor: &PreviewDescriptor,
        offset: u64,
        length: usize,
    ) -> Result<Vec<u8>, FileServiceError> {
        if let Some(resolved) = self.resolved_paths.get(&descriptor.file_id).cloned() {
            if let Some(filesystem) = self.filesystems.get(&resolved.filesystem_key) {
                match filesystem.read_file_range(&resolved.path, offset, length) {
                    Ok(bytes) => return Ok(bytes),
                    Err(error) => tracing::debug!(
                        file_id = %descriptor.file_id,
                        path = %resolved.path,
                        error = %error,
                        "Cached derived-source path failed; resolving again"
                    ),
                }
            }
            self.resolved_paths.remove(&descriptor.file_id);
        }

        let paths = descriptor_image_path_candidates(descriptor);
        let mut failures = Vec::new();
        let candidate = exact_partition_candidate(descriptor)?;
        let key = filesystem_key(candidate)?;
        if !self.filesystems.contains_key(&key) {
            match open_derived_filesystem(runtime, candidate) {
                Ok(filesystem) => {
                    let catalog_fingerprint = crate::derived_source_catalog::catalog_fingerprint(
                        runtime.lineage_fingerprint(),
                    );
                    let locator_scope =
                        derived_filesystem_locator_scope(&catalog_fingerprint, candidate)?;
                    let persisted_counts = restore_filesystem_locators(
                        source_conn,
                        data_source_id,
                        candidate,
                        &locator_scope,
                        filesystem.as_ref(),
                    );
                    self.persisted_locator_counts
                        .insert(key.clone(), persisted_counts);
                    self.filesystem_candidates
                        .insert(key.clone(), candidate.clone());
                    self.filesystem_locator_scopes
                        .insert(key.clone(), locator_scope);
                    self.filesystems.insert(key.clone(), filesystem);
                    self.filesystem_open_operations =
                        self.filesystem_open_operations.saturating_add(1);
                }
                Err(error) => {
                    failures.push(format!(
                        "{} partition {} open failed: {error}",
                        candidate.filesystem_kind, candidate.partition_index
                    ));
                }
            }
        }
        if let Some(filesystem) = self.filesystems.get(&key) {
            for path in &paths {
                match filesystem.read_file_range(path, offset, length) {
                    Ok(bytes) => {
                        self.cache_resolved_path(
                            descriptor.file_id.clone(),
                            ResolvedFilePath {
                                filesystem_key: key.clone(),
                                path: path.clone(),
                            },
                        );
                        return Ok(bytes);
                    }
                    Err(error) => failures.push(format!(
                        "{} partition {} path '{path}': {error}",
                        candidate.filesystem_kind, candidate.partition_index
                    )),
                }
            }
        }

        Err(FileServiceError::other(format!(
            "Ceph RBD file '{}' could not be read from the prepared source: {}",
            descriptor.path,
            failures.join("; ")
        )))
    }

    pub(super) fn flush_filesystem_locators(
        &mut self,
        source_conn: &rusqlite::Connection,
        data_source_id: &domain::DataSourceId,
    ) -> Result<(), FileServiceError> {
        for (key, filesystem) in &self.filesystems {
            let current_counts = RestoredFilesystemLocatorCounts {
                directories: filesystem.directory_locators().len(),
                files: filesystem.file_locators().len(),
            };
            let persisted_counts = self
                .persisted_locator_counts
                .get(key)
                .copied()
                .unwrap_or_default();
            if current_counts.directories <= persisted_counts.directories
                && current_counts.files <= persisted_counts.files
            {
                continue;
            }
            let Some(candidate) = self.filesystem_candidates.get(key) else {
                continue;
            };
            let Some(locator_scope) = self.filesystem_locator_scopes.get(key) else {
                return Err(FileServiceError::other(
                    "Derived filesystem locator scope is missing",
                ));
            };
            persist_filesystem_locators(
                source_conn,
                data_source_id,
                candidate,
                locator_scope,
                filesystem.as_ref(),
            )?;
            self.persisted_locator_counts
                .insert(key.clone(), current_counts);
        }
        Ok(())
    }

    pub(super) fn filesystem_read_metrics(&self) -> evidence_core::FileSystemReadMetrics {
        let mut total = evidence_core::FileSystemReadMetrics {
            filesystem_open_operations: self.filesystem_open_operations,
            ..evidence_core::FileSystemReadMetrics::default()
        };
        for filesystem in self.filesystems.values() {
            total.merge(filesystem.read_metrics());
        }
        total
    }

    fn cache_resolved_path(&mut self, file_id: String, resolved: ResolvedFilePath) {
        if self.resolved_paths.len() >= MAX_RESOLVED_PATH_CACHE_ENTRIES
            && !self.resolved_paths.contains_key(&file_id)
        {
            self.resolved_paths.clear();
        }
        self.resolved_paths.insert(file_id, resolved);
    }
}

fn filesystem_key(candidate: &PreviewPartitionCandidate) -> Result<String, FileServiceError> {
    serde_json::to_string(candidate).map_err(|error| {
        FileServiceError::other(format!(
            "Ceph RBD partition identity could not be serialized: {error}"
        ))
    })
}

#[cfg(test)]
#[path = "../../../tests/unit/file_service/source_read/derived_cache.rs"]
mod tests;
