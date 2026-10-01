//! preview context support for source-bound reads.
use super::*;
impl PreviewReadContext for SourceReadContext<'_> {
    fn conn(&self) -> &rusqlite::Connection {
        self.source_conn
    }

    fn case_id(&self) -> &str {
        &self.case_id.0
    }

    fn get_cached_preview_descriptor(&mut self, key: &str) -> Option<Value> {
        self.descriptors.get(key).cloned()
    }

    fn set_cached_preview_descriptor(&mut self, key: &str, value: &Value) {
        cache_preview_descriptor(&mut self.descriptors, key, value);
    }

    fn open_evidence_reader(
        &mut self,
        descriptor: &PreviewDescriptor,
    ) -> Result<Box<dyn evidence_core::EvidenceReader>, FileServiceError> {
        if descriptor.data_source_id != self.data_source_id.0 {
            return Err(FileServiceError::security(
                "Evidence descriptor does not belong to the bound data source",
            ));
        }
        if descriptor.source_kind == "ceph_rbd" {
            return self
                .derived_runtime()?
                .open_reader()
                .map(|reader| Box::new(reader) as Box<dyn evidence_core::EvidenceReader>)
                .map_err(|error| FileServiceError::other(error.to_string()));
        }
        open_host_evidence_reader(
            &descriptor.source_kind,
            Path::new(&descriptor.source_path),
            &self.case_id.0,
        )
    }

    fn open_candidate_block_reader(
        &mut self,
        descriptor: &PreviewDescriptor,
        candidate: &crate::file_service::viewer::PreviewPartitionCandidate,
    ) -> Result<(Box<dyn evidence_core::EvidenceReader>, u64, String), FileServiceError> {
        bitlocker::open_candidate_block_reader(self, descriptor, candidate)
    }

    fn is_bitlocker_candidate(
        &self,
        candidate: &crate::file_service::viewer::PreviewPartitionCandidate,
    ) -> Result<bool, FileServiceError> {
        bitlocker::is_bitlocker_candidate(self, candidate)
    }

    fn read_cephfs_range(
        &mut self,
        descriptor: &PreviewDescriptor,
        offset: u64,
        length: usize,
    ) -> Result<Option<Vec<u8>>, FileServiceError> {
        if descriptor.source_kind != "ceph_fs" {
            return Ok(None);
        }
        if descriptor.data_source_id != self.data_source_id.0 {
            return Err(FileServiceError::security(
                "CephFS descriptor does not belong to the bound data source",
            ));
        }
        let cephfs = descriptor.ceph_fs.as_ref().ok_or_else(|| {
            FileServiceError::other("CephFS preview descriptor is missing its file locator")
        })?;
        let cache_key = descriptor.file_id.clone();
        if self
            .cephfs_readers
            .get(&cache_key)
            .is_some_and(|reader| reader.projection_sha256() != cephfs.projection_sha256)
        {
            self.cephfs_readers.remove(&cache_key);
        }
        if !self.cephfs_readers.contains_key(&cache_key) {
            if self.cephfs_readers.len() >= MAX_CEPHFS_PREPARED_READERS {
                self.cephfs_readers.clear();
            }
            let reader = open_cephfs_file_reader(
                self.case_conn,
                self.case_root,
                self.case_id,
                self.data_source_id,
                &crate::file_service::cephfs_adapter::file_read_request(descriptor)?,
            )?;
            self.cephfs_readers.insert(cache_key.clone(), reader);
        }
        self.cephfs_readers
            .get_mut(&cache_key)
            .ok_or_else(|| FileServiceError::other("CephFS reader cache insertion failed"))?
            .read_range(offset, length)
            .map(Some)
            .map_err(Into::into)
    }
}
