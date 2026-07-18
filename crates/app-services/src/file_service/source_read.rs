use std::{collections::HashMap, path::Path, sync::Arc};

use domain::{CaseId, DataSourceId, EntryType, FileEntry, FileEntryId};
use persistence_sqlite::repositories::file_repo::FileRepo;
use serde_json::Value;

use crate::{
    ceph_reconstruction::{build_derived_rbd_runtime, DerivedRbdRuntime},
    file_service::{
        viewer::{
            descriptor_for_file_with_cache, open_host_evidence_reader,
            read_file_header_with_context, PreviewDescriptor, PreviewReadContext,
        },
        FileServiceError,
    },
};

mod derived_cache;

use derived_cache::DerivedSourceReadCache;

const MAX_SOURCE_DESCRIPTOR_CACHE_ENTRIES: usize = 4_096;
const MAX_SOURCE_PARTITION_CACHE_ENTRIES: usize = 64;

#[derive(Debug, Clone)]
struct SourceReadFileHint {
    file_id: FileEntryId,
    data_source_id: DataSourceId,
    partition_index: Option<usize>,
    path: String,
    size: u64,
}

impl SourceReadFileHint {
    fn new(
        file_id: FileEntryId,
        data_source_id: DataSourceId,
        partition_index: Option<usize>,
        path: String,
        size: u64,
    ) -> Self {
        Self {
            file_id,
            data_source_id,
            partition_index,
            path,
            size,
        }
    }
}

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
    source_location: Option<(String, String)>,
    partition_candidates:
        HashMap<usize, Vec<crate::file_service::viewer::PreviewPartitionCandidate>>,
    derived_runtime: Option<Arc<DerivedRbdRuntime>>,
    derived_reads: DerivedSourceReadCache,
}

/// Per-worker state for reading a derived source through one shared runtime.
///
/// The state owns only descriptor metadata. Expensive RBD provider, evidence
/// reader, plan-cache, and verified-page state stay shared in `derived_runtime`.
pub(crate) struct PreparedSourceReadState {
    case_id: String,
    data_source_id: DataSourceId,
    descriptors: HashMap<String, Value>,
    derived_runtime: Arc<DerivedRbdRuntime>,
    derived_reads: DerivedSourceReadCache,
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
            source_location: None,
            partition_candidates: HashMap::new(),
            derived_runtime: None,
            derived_reads: DerivedSourceReadCache::default(),
        }
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
        file_id: &FileEntryId,
        data_source_id: &DataSourceId,
        partition_index: Option<usize>,
        path: &str,
        size: u64,
        max_bytes: usize,
    ) -> Result<Vec<u8>, FileServiceError> {
        let hint = SourceReadFileHint::new(
            file_id.clone(),
            data_source_id.clone(),
            partition_index,
            path.to_string(),
            size,
        );
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

    fn descriptor_for_hint(
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
        })
    }

    fn source_location(&mut self) -> Result<&(String, String), FileServiceError> {
        if self.source_location.is_none() {
            self.source_location =
                FileRepo::new(self.source_conn).find_data_source_location(self.data_source_id)?;
        }
        self.source_location
            .as_ref()
            .ok_or_else(|| FileServiceError::not_found("Data source not found"))
    }

    fn partition_candidates_for(
        &mut self,
        hint: &SourceReadFileHint,
        partition_index: usize,
        source_kind: &str,
        source_path: &str,
    ) -> Result<Vec<crate::file_service::viewer::PreviewPartitionCandidate>, FileServiceError> {
        if let Some(cached) = self.partition_candidates.get(&partition_index) {
            return Ok(cached.clone());
        }
        let entry = hint_file_entry(hint);
        let candidates = match source_kind {
            "e01" | "ceph_rbd" => crate::file_service::viewer::e01_partition_candidates(
                self.source_conn,
                &entry,
                Some(partition_index),
            )?,
            "raw" => crate::file_service::viewer::raw_partition_candidates(
                source_path,
                Some(partition_index),
            )?,
            "logical_directory" => Vec::new(),
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

impl PreparedSourceReadState {
    pub(crate) fn new(
        case_id: impl Into<String>,
        data_source_id: DataSourceId,
        derived_runtime: Arc<DerivedRbdRuntime>,
    ) -> Self {
        Self {
            case_id: case_id.into(),
            data_source_id,
            descriptors: HashMap::new(),
            derived_runtime,
            derived_reads: DerivedSourceReadCache::default(),
        }
    }

    pub(crate) fn read_file_header_by_id(
        &mut self,
        source_conn: &rusqlite::Connection,
        file_id: &FileEntryId,
        max_bytes: usize,
    ) -> Result<Vec<u8>, FileServiceError> {
        let descriptor = {
            let mut context = PreparedSourceReadContext {
                source_conn,
                state: self,
            };
            descriptor_for_file_with_cache(&mut context, file_id)?
        };
        self.derived_reads.read_file_header(
            source_conn,
            &self.data_source_id,
            &self.derived_runtime,
            &descriptor,
            max_bytes,
        )
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
}

struct PreparedSourceReadContext<'a> {
    source_conn: &'a rusqlite::Connection,
    state: &'a mut PreparedSourceReadState,
}

impl PreviewReadContext for PreparedSourceReadContext<'_> {
    fn conn(&self) -> &rusqlite::Connection {
        self.source_conn
    }

    fn case_id(&self) -> &str {
        &self.state.case_id
    }

    fn get_cached_preview_descriptor(&mut self, key: &str) -> Option<Value> {
        self.state.descriptors.get(key).cloned()
    }

    fn set_cached_preview_descriptor(&mut self, key: &str, value: &Value) {
        cache_preview_descriptor(&mut self.state.descriptors, key, value);
    }

    fn open_evidence_reader(
        &mut self,
        descriptor: &PreviewDescriptor,
    ) -> Result<Box<dyn evidence_core::EvidenceReader>, FileServiceError> {
        if descriptor.data_source_id != self.state.data_source_id.0 {
            return Err(FileServiceError::security(
                "Evidence descriptor does not belong to the bound data source",
            ));
        }
        if descriptor.source_kind != "ceph_rbd" {
            return Err(FileServiceError::security(
                "Prepared derived-source reader received a non-RBD descriptor",
            ));
        }
        self.state
            .derived_runtime
            .open_reader()
            .map(|reader| Box::new(reader) as Box<dyn evidence_core::EvidenceReader>)
            .map_err(|error| FileServiceError::other(error.to_string()))
    }
}

fn cache_preview_descriptor(cache: &mut HashMap<String, Value>, key: &str, value: &Value) {
    if cache.len() >= MAX_SOURCE_DESCRIPTOR_CACHE_ENTRIES && !cache.contains_key(key) {
        cache.clear();
    }
    cache.insert(key.to_string(), value.clone());
}

fn cache_partition_candidates(
    cache: &mut HashMap<usize, Vec<crate::file_service::viewer::PreviewPartitionCandidate>>,
    partition_index: usize,
    candidates: Vec<crate::file_service::viewer::PreviewPartitionCandidate>,
) {
    if cache.len() >= MAX_SOURCE_PARTITION_CACHE_ENTRIES && !cache.contains_key(&partition_index) {
        cache.clear();
    }
    cache.insert(partition_index, candidates);
}

fn hint_file_entry(hint: &SourceReadFileHint) -> FileEntry {
    let name = Path::new(&hint.path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(&hint.path)
        .to_string();
    FileEntry {
        id: hint.file_id.clone(),
        parent_id: None,
        data_source_id: hint.data_source_id.clone(),
        path: hint.path.clone(),
        name,
        entry_type: EntryType::File,
        size: Some(hint.size),
        ext: None,
        deleted: false,
        hidden: false,
        system: false,
        encrypted: false,
        created_at: None,
        modified_at: None,
        accessed_at: None,
        changed_at: None,
        hash_sha256: None,
    }
}

#[cfg(test)]
#[path = "../../tests/unit/file_service/source_read.rs"]
mod tests;
