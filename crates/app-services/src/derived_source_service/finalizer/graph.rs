use std::{
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use domain::{CaseId, DataSourceId};
use persistence_sqlite::repositories::processing_phase_repo::ProcessingPhase;
use serde_json::json;

use super::{
    outcome::DerivedFinalizationReport, phase_execution::run_cancellable_phase,
    phase_runner::ProcessingPhaseRunner,
};
use crate::source_db;

pub(super) fn run_graph_phase(
    runner: &ProcessingPhaseRunner<'_>,
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
    cancel_token: &Arc<AtomicBool>,
    report: &mut DerivedFinalizationReport,
) {
    run_cancellable_phase(runner, ProcessingPhase::Graph, report, cancel_token, || {
        ensure_not_cancelled(cancel_token)?;
        let source =
            source_db::open_ready_source_by_id(case_conn, case_root, case_id, data_source_id)
                .map_err(|error| error.to_string())?;
        crate::file_service::populate_file_graph_for_data_source(
            &source.connection,
            data_source_id,
        )
        .map_err(|error| error.to_string())?;
        ensure_not_cancelled(cancel_token)?;
        source_db::checkpoint_source_db(&source.connection).map_err(|error| error.to_string())?;
        Ok(json!({"projected": true}).to_string())
    });
}

fn ensure_not_cancelled(cancel_token: &AtomicBool) -> Result<(), String> {
    if cancel_token.load(Ordering::Relaxed) {
        Err("Derived-source graph projection cancelled".to_string())
    } else {
        Ok(())
    }
}
