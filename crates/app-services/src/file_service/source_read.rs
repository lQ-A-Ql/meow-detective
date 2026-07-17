use std::{collections::HashMap, path::Path, sync::Arc};

use domain::{CaseId, DataSourceId, FileEntryId};
use serde_json::Value;

use crate::{
    ceph_reconstruction::{build_derived_rbd_runtime, DerivedRbdRuntime},
    file_service::{
        viewer::{
            open_host_evidence_reader, read_file_header_with_context, PreviewDescriptor,
            PreviewReadContext,
        },
        FileServiceError,
    },
};

/// Source-bound evidence reader shared by preview, analysis, and reporting.
///
/// The context binds a source database to its case registration and keeps a
/// single derived RBD runtime alive for the duration of the caller's use case.
pub(crate) struct SourceReadContext<'a> {
    source_conn: &'a rusqlite::Connection,
    case_conn: &'a rusqlite::Connection,
    case_root: &'a Path,
    case_id: &'a CaseId,
    data_source_id: &'a DataSourceId,
    descriptors: HashMap<String, Value>,
    derived_runtime: Option<Arc<DerivedRbdRuntime>>,
}

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
            derived_runtime: None,
        }
    }

    pub(crate) fn read_file_header_by_id(
        &mut self,
        file_id: &FileEntryId,
        max_bytes: usize,
    ) -> Result<Vec<u8>, FileServiceError> {
        read_file_header_with_context(self, file_id, max_bytes)
    }

    fn derived_runtime(&mut self) -> Result<&Arc<DerivedRbdRuntime>, FileServiceError> {
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
}

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
        self.descriptors.insert(key.to_string(), value.clone());
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
}
