use std::sync::Mutex;

use crate::{
    ceph_reconstruction::PreparedCephFsFileReader,
    file_service::{
        preview_runtime::prepared_ceph::{PreparedCephFile, SharedPreparedFilesystem},
        viewer::PreviewDescriptor,
        FileServiceError,
    },
};

enum PreparedPreview {
    Rbd(Mutex<PreparedCephFile>),
    CephFs(Box<Mutex<PreparedCephFsFileReader>>),
}

pub(crate) struct PreviewSession {
    case_id: String,
    data_source_id: String,
    global_file_id: String,
    size: u64,
    mime: Option<String>,
    runtime_fingerprint: Option<String>,
    prepared: Option<PreparedPreview>,
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
            prepared: None,
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
            prepared: Some(PreparedPreview::Rbd(Mutex::new(prepared_ceph))),
        })
    }

    pub(crate) fn prepared_cephfs(
        case_id: String,
        global_file_id: String,
        size: u64,
        mime: Option<String>,
        descriptor: &PreviewDescriptor,
        reader: PreparedCephFsFileReader,
    ) -> Self {
        let runtime_fingerprint = reader.lineage_fingerprint().to_string();
        Self {
            case_id,
            data_source_id: descriptor.data_source_id.clone(),
            global_file_id,
            size,
            mime,
            runtime_fingerprint: Some(runtime_fingerprint),
            prepared: Some(PreparedPreview::CephFs(Box::new(Mutex::new(reader)))),
        }
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
        let Some(prepared) = &self.prepared else {
            return Ok(None);
        };
        match prepared {
            PreparedPreview::Rbd(prepared) => prepared
                .lock()
                .map_err(|_| FileServiceError::other("Preview session lock is poisoned"))?
                .read_range(offset, length)
                .map(Some),
            PreparedPreview::CephFs(prepared) => prepared
                .lock()
                .map_err(|_| FileServiceError::other("CephFS preview session lock is poisoned"))?
                .read_range(offset, length)
                .map(Some)
                .map_err(Into::into),
        }
    }
}
