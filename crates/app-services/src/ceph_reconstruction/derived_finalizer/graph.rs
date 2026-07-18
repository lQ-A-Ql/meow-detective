use std::path::Path;

use domain::{CaseId, DataSourceId};
use persistence_sqlite::repositories::processing_phase_repo::ProcessingPhase;
use serde_json::json;

use super::{
    outcome::DerivedFinalizationReport, phase_execution::run_phase,
    phase_runner::ProcessingPhaseRunner,
};
use crate::source_db;

pub(super) fn run_graph_phase(
    runner: &ProcessingPhaseRunner<'_>,
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
    report: &mut DerivedFinalizationReport,
) {
    run_phase(runner, ProcessingPhase::Graph, report, || {
        let source =
            source_db::open_ready_source_by_id(case_conn, case_root, case_id, data_source_id)
                .map_err(|error| error.to_string())?;
        crate::file_service::populate_file_graph_for_data_source(
            &source.connection,
            data_source_id,
        )
        .map_err(|error| error.to_string())?;
        source_db::checkpoint_source_db(&source.connection).map_err(|error| error.to_string())?;
        Ok(json!({"projected": true}).to_string())
    });
}
