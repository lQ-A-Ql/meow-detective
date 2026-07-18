use std::sync::Mutex;

use crate::file_service::{
    preview_runtime::prepared_ceph::{PreparedCephFile, SharedPreparedFilesystem},
    viewer::PreviewDescriptor,
    FileServiceError,
};

pub(crate) struct PreviewSession {
    case_id: String,
    data_source_id: String,
    global_file_id: String,
    size: u64,
    mime: Option<String>,
    runtime_fingerprint: Option<String>,
    prepared_ceph: Option<Mutex<PreparedCephFile>>,
}

impl PreviewSession {
    pub(crate) fn routed(
        case_id: String,
        data_source_id: String,
        global_file_id: String,
        size: u64,
        mime: Option<String>,
    ) -> Self {
        Self {
            case_id,
            data_source_id,
            global_file_id,
            size,
            mime,
            runtime_fingerprint: None,
            prepared_ceph: None,
        }
    }

    pub(crate) fn prepared_ceph(
        case_id: String,
        global_file_id: String,
        size: u64,
        mime: Option<String>,
        runtime_fingerprint: String,
        filesystem: SharedPreparedFilesystem,
        descriptor: &PreviewDescriptor,
    ) -> Result<Self, FileServiceError> {
        let data_source_id = descriptor.data_source_id.clone();
        let prepared_ceph = PreparedCephFile::open(filesystem, descriptor)?;
        Ok(Self {
            case_id,
            data_source_id,
            global_file_id,
            size,
            mime,
            runtime_fingerprint: Some(runtime_fingerprint),
            prepared_ceph: Some(Mutex::new(prepared_ceph)),
        })
    }

    pub(crate) fn case_id(&self) -> &str {
        &self.case_id
    }

    pub(crate) fn data_source_id(&self) -> &str {
        &self.data_source_id
    }

    pub(crate) fn global_file_id(&self) -> &str {
        &self.global_file_id
    }

    pub(crate) fn size(&self) -> u64 {
        self.size
    }

    pub(crate) fn mime(&self) -> Option<&str> {
        self.mime.as_deref()
    }

    pub(crate) fn runtime_fingerprint(&self) -> Option<&str> {
        self.runtime_fingerprint.as_deref()
    }

    pub(crate) fn read_prepared_range(
        &self,
        offset: u64,
        length: usize,
    ) -> Result<Option<Vec<u8>>, FileServiceError> {
        let Some(prepared) = &self.prepared_ceph else {
            return Ok(None);
        };
        let mut prepared = prepared
            .lock()
            .map_err(|_| FileServiceError::other("Preview session lock is poisoned"))?;
        prepared.read_range(offset, length).map(Some)
    }
}
