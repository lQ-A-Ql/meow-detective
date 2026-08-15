use super::Result;
use domain::CaseId;
use persistence_sqlite::repositories::{
    artifact_repo::ArtifactRepo, case_repo::CaseMetrics, datasource_repo::DataSourceRepo,
    file_repo::FileRepo, timeline_repo::TimelineRepo,
};
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
    for (_, source_conn) in
        crate::source_db::open_ready_source_connections_read_only(conn, case_root, case_id)?
    {
        metrics.indexed_file_count = metrics
            .indexed_file_count
            .saturating_add(FileRepo::new(&source_conn).count_all()?);
        metrics.timeline_event_count = metrics
            .timeline_event_count
            .saturating_add(TimelineRepo::new(&source_conn).count()?);
        metrics.artifact_count = metrics
            .artifact_count
            .saturating_add(ArtifactRepo::new(&source_conn).count()?);
    }
    Ok(metrics)
}
