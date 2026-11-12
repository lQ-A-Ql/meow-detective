use domain::{DataSourceId, DataSourcePlatform};
use persistence_sqlite::repositories::{
    datasource_repo::DataSourceRepo, processing_phase_repo::ProcessingPhase,
};
use serde_json::json;

use super::{
    outcome::DerivedFinalizationReport, phase_execution::run_phase,
    phase_runner::ProcessingPhaseRunner,
};

pub(super) fn run_platform_phase(
    runner: &ProcessingPhaseRunner<'_>,
    platform: DataSourcePlatform,
    report: &mut DerivedFinalizationReport,
) -> persistence_sqlite::repositories::processing_phase_repo::ProcessingPhaseState {
    run_phase(runner, ProcessingPhase::Platform, report, || {
        Ok(json!({
            "platform": platform.as_storage_str(),
            "detector": "registered-source-platform",
        })
        .to_string())
    })
}

pub(super) fn resolve_platform(
    case_conn: &rusqlite::Connection,
    data_source_id: &DataSourceId,
) -> Result<DataSourcePlatform, String> {
    let storage = DataSourceRepo::new(case_conn)
        .find_storage(data_source_id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "derived source storage metadata is missing".to_string())?;
    DataSourcePlatform::from_storage_str(Some(&storage.platform)).map_err(|error| error.to_string())
}
