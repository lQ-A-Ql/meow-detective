use std::sync::Arc;

use domain::{CaseId, DataSourceKind, FileEntryId};
use persistence_sqlite::repositories::datasource_repo::DataSourceRepo;
use rusqlite::Connection;

use super::{ImportAnalysisOptions, PostImportPipelineOptions};
use crate::file_service;

pub(super) enum AnalysisSourceReader {
    Host(file_service::FileHeaderReadCache),
    Derived(file_service::PreparedSourceReadState),
}

impl AnalysisSourceReader {
    pub(super) fn for_options(
        options: &ImportAnalysisOptions,
        derived_runtime: Option<Arc<crate::ceph_reconstruction::DerivedRbdRuntime>>,
    ) -> Self {
        match derived_runtime {
            Some(runtime) => Self::Derived(file_service::PreparedSourceReadState::new(
                options.case_id.clone(),
                options.data_source_id.clone(),
                runtime,
            )),
            None => Self::Host(file_service::FileHeaderReadCache::new(
                options.case_id.clone(),
            )),
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
    if !options.enable_content_extraction && !options.enable_text_indexing {
        return Ok(None);
    }
    let source_conn = persistence_sqlite::open_or_create(&options.db_path)
        .map_err(|error| format!("Open analysis source database: {error}"))?;
    let source_kind = DataSourceRepo::new(&source_conn)
        .source_kind(&options.data_source_id)
        .map_err(|error| format!("Resolve analysis source kind: {error}"))?;
    if source_kind != DataSourceKind::CephRbd {
        return Ok(None);
    }

    let case_conn = crate::connection::open_case_db(&options.case_root.join("app.db"))
        .map_err(|error| format!("Open case database for derived analysis: {error}"))?;
    crate::ceph_reconstruction::build_derived_rbd_runtime(
        &case_conn,
        &options.case_root,
        &CaseId(options.case_id.clone()),
        &options.data_source_id,
    )
    .map(Arc::new)
    .map(Some)
    .map_err(|error| format!("Prepare derived RBD analysis runtime: {error}"))
}
