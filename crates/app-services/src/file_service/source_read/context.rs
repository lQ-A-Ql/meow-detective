//! context support for source-bound reads.
use super::*;
impl<'a> SourceReadContext<'a> {
    pub(crate) fn new(
        source_conn: &'a rusqlite::Connection,
        case_conn: &'a rusqlite::Connection,
        case_root: &'a Path,
        case_id: &'a CaseId,
        data_source_id: &'a DataSourceId,
    ) -> Self {
        Self {
            source_conn,
            case_conn,
            case_root,
            case_id,
            data_source_id,
            descriptors: HashMap::new(),
            source_location: None,
            partition_candidates: HashMap::new(),
            derived_runtime: None,
            derived_reads: DerivedSourceReadCache::default(),
            cephfs_readers: HashMap::new(),
            bitlocker_runtime: None,
        }
    }

    pub(crate) fn source_connection(&self) -> &rusqlite::Connection {
        self.source_conn
    }

    pub(crate) fn read_file_header_by_id(
        &mut self,
        file_id: &FileEntryId,
        max_bytes: usize,
    ) -> Result<Vec<u8>, FileServiceError> {
        let descriptor = descriptor_for_file_with_cache(&mut *self, file_id)?;
        if descriptor.source_kind == "ceph_rbd" {
            let runtime = self.derived_runtime()?.clone();
            return self.derived_reads.read_file_header(
                self.source_conn,
                self.data_source_id,
                &runtime,
                &descriptor,
                max_bytes,
            );
        }
        read_file_header_with_context(self, file_id, max_bytes)
    }

    pub(crate) fn read_file_header_with_metadata(
        &mut self,
        hint: SourceReadFileHint,
        max_bytes: usize,
    ) -> Result<Vec<u8>, FileServiceError> {
        if hint.data_source_id != *self.data_source_id {
            return Err(FileServiceError::security(
                "Source-read hint does not belong to the bound data source",
            ));
        }
        metadata::validate_hint_encryption(self.source_conn, &hint)?;
        let Some(partition_index) = hint.partition_index else {
            return self.read_file_header_by_id(&hint.file_id, max_bytes);
        };
        let (source_kind, source_path) = self.source_location()?.clone();
        if source_kind != "ceph_rbd" {
            return self.read_file_header_by_id(&hint.file_id, max_bytes);
        }
        let descriptor =
            self.descriptor_for_hint(&hint, partition_index, source_kind, source_path)?;
        let runtime = self.derived_runtime()?.clone();
        self.derived_reads.read_file_header(
            self.source_conn,
            self.data_source_id,
            &runtime,
            &descriptor,
            max_bytes,
        )
    }

    pub(crate) fn flush_derived_filesystem_locators(&mut self) -> Result<(), FileServiceError> {
        self.derived_reads
            .flush_filesystem_locators(self.source_conn, self.data_source_id)
    }

    pub(crate) fn filesystem_read_metrics(&self) -> evidence_core::FileSystemReadMetrics {
        self.derived_reads.filesystem_read_metrics()
    }

    pub(crate) fn rados_read_metrics(
        &self,
    ) -> crate::ceph_reconstruction::RadosProviderReadMetrics {
        self.derived_runtime
            .as_ref()
            .map(|runtime| runtime.read_metrics())
            .unwrap_or_default()
    }

    pub(super) fn derived_runtime(&mut self) -> Result<&Arc<DerivedRbdRuntime>, FileServiceError> {
        if self.derived_runtime.is_none() {
            let runtime = build_derived_rbd_runtime(
                self.case_conn,
                self.case_root,
                self.case_id,
                self.data_source_id,
            )
            .map(Arc::new)
            .map_err(|error| FileServiceError::other(error.to_string()))?;
            self.derived_runtime = Some(runtime);
        }
        self.derived_runtime
            .as_ref()
            .ok_or_else(|| FileServiceError::other("Derived RBD runtime was not initialized"))
    }

    pub(super) fn descriptor_for_hint(
        &mut self,
        hint: &SourceReadFileHint,
        partition_index: usize,
        source_kind: String,
        source_path: String,
    ) -> Result<PreviewDescriptor, FileServiceError> {
        if hint.data_source_id != *self.data_source_id {
            return Err(FileServiceError::security(
                "Source-read hint does not belong to the bound data source",
            ));
        }
        let partition_candidates =
            self.partition_candidates_for(hint, partition_index, &source_kind, &source_path)?;
        let [selected] = partition_candidates.as_slice() else {
            return Err(FileServiceError::other(format!(
                "Source-read descriptor requires exactly one partition candidate, found {}",
                partition_candidates.len()
            )));
        };
        Ok(PreviewDescriptor {
            case_id: self.case_id.0.clone(),
            file_id: hint.file_id.0.clone(),
            source_kind,
            source_path,
            partition_index: Some(selected.partition_index),
            filesystem_kind: Some(selected.filesystem_kind.clone()),
            path: hint.path.clone(),
            mime: None,
            size: hint.size,
            data_source_id: hint.data_source_id.0.clone(),
            partition_candidates,
            entry_size: hint.size,
            entry_modified_at: None,
            ceph_fs: None,
        })
    }

    pub(super) fn source_location(&mut self) -> Result<&(String, String), FileServiceError> {
        if self.source_location.is_none() {
            self.source_location =
                FileRepo::new(self.source_conn).find_data_source_location(self.data_source_id)?;
        }
        self.source_location
            .as_ref()
            .ok_or_else(|| FileServiceError::not_found("Data source not found"))
    }

    pub(super) fn partition_candidates_for(
        &mut self,
        hint: &SourceReadFileHint,
        partition_index: usize,
        source_kind: &str,
        source_path: &str,
    ) -> Result<Vec<crate::file_service::viewer::PreviewPartitionCandidate>, FileServiceError> {
        if let Some(cached) = self.partition_candidates.get(&partition_index) {
            return Ok(cached.clone());
        }
        let entry = metadata::hint_file_entry(hint);
        let candidates = match source_kind {
            "e01" | "ceph_rbd" => crate::file_service::viewer::e01_partition_candidates(
                self.source_conn,
                &entry,
                Some(partition_index),
            )?,
            "raw" | "local_disk" | "android_sparse" => {
                crate::file_service::viewer::block_partition_candidates(
                    source_path,
                    Some(partition_index),
                    source_kind,
                )?
            }
            "logical_directory" | "logical_archive" => Vec::new(),
            other => {
                return Err(FileServiceError::other(format!(
                    "Range reading is not yet wired for data source kind '{other}'"
                )))
            }
        };
        cache_partition_candidates(
            &mut self.partition_candidates,
            partition_index,
            candidates.clone(),
        );
        Ok(candidates)
    }
}
