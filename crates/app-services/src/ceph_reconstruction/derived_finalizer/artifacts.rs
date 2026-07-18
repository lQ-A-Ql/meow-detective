use std::path::Path;

use domain::{CaseId, DataSourceId, DataSourcePlatform};
use persistence_sqlite::repositories::processing_phase_repo::ProcessingPhase;
use serde_json::json;

use super::{
    outcome::DerivedFinalizationReport, phase_execution::run_phase,
    phase_runner::ProcessingPhaseRunner,
};
use crate::analysis_service::run_source_analysis_extraction;

pub(super) fn run_artifact_phase(
    runner: &ProcessingPhaseRunner<'_>,
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
    platform: DataSourcePlatform,
    report: &mut DerivedFinalizationReport,
) -> persistence_sqlite::repositories::processing_phase_repo::ProcessingPhaseState {
    run_phase(runner, ProcessingPhase::Artifacts, report, || {
        let categories = categories_for(platform)?;
        let extraction = run_source_analysis_extraction(
            case_conn,
            case_root,
            case_id,
            data_source_id,
            &categories,
        )
        .map_err(|error| error.to_string())?;
        Ok(json!({
            "status": extraction.status,
            "scannedCount": extraction.scanned_count,
            "artifactCount": extraction.artifact_count,
            "timelineEventCount": extraction.timeline_event_count,
            "warningCount": extraction.warnings.len(),
        })
        .to_string())
    })
}

fn categories_for(platform: DataSourcePlatform) -> Result<Vec<&'static str>, String> {
    match platform {
        DataSourcePlatform::Linux => Ok(vec!["LinuxArtifacts"]),
        DataSourcePlatform::Windows => Ok(Vec::new()),
        DataSourcePlatform::Unknown => {
            Err("unknown guest platform cannot run artifact extraction".to_string())
        }
    }
}
