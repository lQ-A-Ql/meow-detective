use super::Result;
use domain::CaseId;
use persistence_sqlite::repositories::{case_repo::CaseMetrics, datasource_repo::DataSourceRepo};
use persistence_sqlite::DbError;
use rusqlite::Connection;
use std::path::Path;

pub fn get_case_metrics_for_case(
    conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
) -> Result<CaseMetrics> {
    let sources = DataSourceRepo::new(conn).find_by_case(case_id)?;
    let mut metrics = CaseMetrics {
        data_source_count: sources.len() as u64,
        indexed_file_count: 0,
        timeline_event_count: 0,
        artifact_count: 0,
    };
    for (source_id, _source_conn) in
        crate::source_db::open_ready_source_connections_read_only(conn, case_root, case_id)?
    {
        let catalog_count = crate::source_db::ready_source_catalog_count(case_root, &source_id)
            .map_err(|error| DbError::System(error.to_string()))?;
        metrics.indexed_file_count = metrics.indexed_file_count.saturating_add(catalog_count);
        let timeline_count = crate::source_db::ready_source_timeline_count(case_root, &source_id)
            .map_err(|error| DbError::System(error.to_string()))?;
        metrics.timeline_event_count = metrics.timeline_event_count.saturating_add(timeline_count);
        let artifact_count = crate::source_db::ready_source_artifact_count(case_root, &source_id)
            .map_err(|error| DbError::System(error.to_string()))?;
        metrics.artifact_count = metrics.artifact_count.saturating_add(artifact_count);
    }
    Ok(metrics)
}
