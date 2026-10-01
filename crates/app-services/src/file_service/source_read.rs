use std::{collections::HashMap, path::Path, sync::Arc};

use domain::{CaseId, DataSourceId, FileEntryId};
use persistence_sqlite::repositories::file_repo::FileRepo;
use serde_json::Value;

use crate::{
    ceph_reconstruction::{
        build_derived_rbd_runtime, open_cephfs_file_reader, DerivedRbdRuntime,
        PreparedCephFsFileReader,
    },
    file_service::{
        viewer::{
            descriptor_for_file_with_cache, open_host_evidence_reader,
            read_file_header_with_context, PreviewDescriptor, PreviewReadContext,
        },
        FileServiceError,
    },
};
mod bitlocker;
mod cache;
mod context;
mod derived_cache;
mod hint;
mod metadata;
mod parallel;
mod prepared;
mod preview_context;
mod stream;

use cache::{cache_partition_candidates, cache_preview_descriptor};
use derived_cache::DerivedSourceReadCache;
pub(crate) use parallel::ParallelSourceReaders;
pub(crate) use stream::SourceExtractionMode;

const MAX_SOURCE_DESCRIPTOR_CACHE_ENTRIES: usize = 4_096;
const MAX_SOURCE_PARTITION_CACHE_ENTRIES: usize = 64;
const MAX_CEPHFS_PREPARED_READERS: usize = 8;

#[derive(Debug, Clone)]
pub(crate) struct SourceReadFileHint {
    file_id: FileEntryId,
    data_source_id: DataSourceId,
    partition_index: Option<usize>,
    path: String,
    size: u64,
    encrypted: bool,
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
    cephfs_readers: HashMap<String, PreparedCephFsFileReader>,
    bitlocker_runtime: Option<Arc<crate::bitlocker_runtime::BitLockerUnlockRegistry>>,
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

#[cfg(test)]
#[path = "../../tests/unit/file_service/source_read.rs"]
mod tests;
