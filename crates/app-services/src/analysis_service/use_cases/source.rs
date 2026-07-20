use std::path::Path;

use domain::{CaseId, DataSourceId, DataSourceKind, DataSourcePlatform};
use persistence_sqlite::repositories::datasource_repo::DataSourceRepo;
use rusqlite::Connection;

use crate::analysis_service::AnalysisServiceError;
use crate::source_db;

pub(super) struct AnalysisSource {
    pub(super) connection: Connection,
    pub(super) platform: DataSourcePlatform,
}

pub(super) fn open_ready_analysis_source(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
) -> Result<AnalysisSource, AnalysisServiceError> {
    let source_kind = DataSourceRepo::new(case_conn).source_kind(data_source_id)?;
    if source_kind == DataSourceKind::CephFs {
        return Err(AnalysisServiceError::Unsupported(
            "CephFS does not run host-platform artifact extraction".to_string(),
        ));
    }
    let source = source_db::open_ready_source_by_id(case_conn, case_root, case_id, data_source_id)?;
    Ok(AnalysisSource {
        connection: source.connection,
        platform: source.platform,
    })
}
