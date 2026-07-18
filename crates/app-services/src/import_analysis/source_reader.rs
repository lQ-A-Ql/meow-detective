use std::sync::Arc;

use domain::{CaseId, DataSourceKind, FileEntryId};
use persistence_sqlite::repositories::datasource_repo::DataSourceRepo;
use rusqlite::Connection;

use super::{ImportAnalysisOptions, PostImportPipelineOptions};
use crate::file_service;

pub(super) enum AnalysisSourceReader {
    Host(file_service::FileHeaderReadCache),
    Derived(Box<file_service::PreparedSourceReadState>),
}

impl AnalysisSourceReader {
    pub(super) fn for_options(
        options: &ImportAnalysisOptions,
        derived_runtime: Option<Arc<crate::ceph_reconstruction::DerivedRbdRuntime>>,
    ) -> Self {
        Self::for_source(
            options.case_id.clone(),
            options.data_source_id.clone(),
            derived_runtime,
        )
    }

    pub(super) fn for_source(
        case_id: String,
        data_source_id: domain::DataSourceId,
        derived_runtime: Option<Arc<crate::ceph_reconstruction::DerivedRbdRuntime>>,
    ) -> Self {
        match derived_runtime {
            Some(runtime) => Self::Derived(Box::new(file_service::PreparedSourceReadState::new(
                case_id,
                data_source_id,
                runtime,
            ))),
            None => Self::Host(file_service::FileHeaderReadCache::new(case_id)),
        }
    }

    pub(super) fn read_file_header_by_id(
        &mut self,
        conn: &Connection,
        file_id: &FileEntryId,
        max_bytes: usize,
    ) -> Result<Vec<u8>, file_service::FileServiceError> {
        match self {
            Self::Host(cache) => cache.read_file_header_by_id(conn, file_id, max_bytes),
            Self::Derived(state) => state.read_file_header_by_id(conn, file_id, max_bytes),
        }
    }
}

pub(super) fn prepare_derived_runtime(
    options: &PostImportPipelineOptions,
) -> Result<Option<Arc<crate::ceph_reconstruction::DerivedRbdRuntime>>, String> {
    prepare_derived_runtime_for_source(
        &options.case_root,
        &options.db_path,
        &options.case_id,
        &options.data_source_id,
        options.enable_content_extraction || options.enable_text_indexing,
    )
}

pub(super) fn prepare_derived_runtime_for_source(
    case_root: &std::path::Path,
    db_path: &std::path::Path,
    case_id: &str,
    data_source_id: &domain::DataSourceId,
    content_required: bool,
) -> Result<Option<Arc<crate::ceph_reconstruction::DerivedRbdRuntime>>, String> {
    if !content_required {
        return Ok(None);
    }
    let source_conn = persistence_sqlite::open_existing_source_read_only(db_path)
        .map_err(|error| format!("Open analysis source database: {error}"))?;
    let source_kind = DataSourceRepo::new(&source_conn)
        .source_kind(data_source_id)
        .map_err(|error| format!("Resolve analysis source kind: {error}"))?;
    if source_kind != DataSourceKind::CephRbd {
        return Ok(None);
    }

    let case_conn = crate::connection::open_case_db(&case_root.join("app.db"))
        .map_err(|error| format!("Open case database for derived analysis: {error}"))?;
    crate::ceph_reconstruction::build_derived_rbd_runtime(
        &case_conn,
        case_root,
        &CaseId(case_id.to_string()),
        data_source_id,
    )
    .map(Arc::new)
    .map(Some)
    .map_err(|error| format!("Prepare derived RBD analysis runtime: {error}"))
}
