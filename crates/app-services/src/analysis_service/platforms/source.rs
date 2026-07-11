use domain::{CaseId, DataSourceId, DataSourcePlatform};
use rusqlite::Connection;

use crate::analysis_service::error::AnalysisServiceError;

pub fn resolve_data_source_platform(
    case_conn: &Connection,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
) -> Result<DataSourcePlatform, AnalysisServiceError> {
    crate::source_db::resolve_ready_source_platform(case_conn, case_id, data_source_id)
        .map_err(AnalysisServiceError::from)
}

pub fn validate_data_source_analysis_categories(
    case_conn: &Connection,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
    categories: &[&str],
) -> Result<DataSourcePlatform, AnalysisServiceError> {
    let platform = resolve_data_source_platform(case_conn, case_id, data_source_id)?;
    super::analyzer_for(platform)?.select_capabilities(categories)?;
    Ok(platform)
}
