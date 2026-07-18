use std::{path::Path, sync::atomic::AtomicBool, sync::Arc};

use domain::{CaseId, DataSourceId, DataSourcePlatform};
use persistence_sqlite::repositories::processing_phase_repo::ProcessingPhase;
use serde_json::json;

use super::{
    outcome::DerivedFinalizationReport, phase_execution::run_phase,
    phase_runner::ProcessingPhaseRunner,
};
use crate::{
    import_analysis::{self, SearchIndexPhaseOptions},
    source_db, timeline_service,
};

pub(super) fn run_timeline_phase(
    runner: &ProcessingPhaseRunner<'_>,
    case_root: &Path,
    data_source_id: &DataSourceId,
    report: &mut DerivedFinalizationReport,
) {
    run_phase(runner, ProcessingPhase::Timeline, report, || {
        run_timeline_projection(case_root, data_source_id)
    });
}

pub(super) fn run_search_phase(
    runner: &ProcessingPhaseRunner<'_>,
    case_root: &Path,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
    platform: DataSourcePlatform,
    report: &mut DerivedFinalizationReport,
) {
    run_phase(runner, ProcessingPhase::Search, report, || {
        run_search_projection(case_root, case_id, data_source_id, platform)
    });
}

fn run_timeline_projection(
    case_root: &Path,
    data_source_id: &DataSourceId,
) -> Result<String, String> {
    let db_path = source_db::source_db_path(case_root, data_source_id);
    let connection = persistence_sqlite::open_existing_source(&db_path)
        .map_err(|error| format!("Open source database for Timeline phase: {error}"))?;
    let projection = timeline_service::ensure_macb_timeline_projected(&connection)
        .map_err(|error| error.to_string())?;
    let (macb_total_count, artifact_generated_event_count) = timeline_event_counts(&connection)?;
    source_db::checkpoint_source_db(&connection).map_err(|error| error.to_string())?;

    Ok(json!({
        "macbInsertedCount": projection.inserted_count,
        "macbTotalCount": macb_total_count,
        "artifactGeneratedEventCount": artifact_generated_event_count,
        "alreadyProjected": projection.already_projected,
        "warningCount": projection.warnings.len(),
    })
    .to_string())
}

fn run_search_projection(
    case_root: &Path,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
    platform: DataSourcePlatform,
) -> Result<String, String> {
    let stats = import_analysis::run_search_index_phase(SearchIndexPhaseOptions {
        case_root: case_root.to_path_buf(),
        db_path: source_db::source_db_path(case_root, data_source_id),
        case_id: case_id.0.clone(),
        data_source_id: data_source_id.clone(),
        platform,
        index_dir: source_db::source_index_dir(case_root, data_source_id),
        cancel_token: Arc::new(AtomicBool::new(false)),
    })
    .map_err(|error| error.to_string())?;

    Ok(json!({
        "eligibleCount": stats.eligible_count,
        "indexedCount": stats.indexed_count,
        "skippedCount": stats.skipped_count,
        "failedCount": stats.failed_count,
    })
    .to_string())
}

pub(super) fn timeline_event_counts(
    connection: &rusqlite::Connection,
) -> Result<(u64, u64), String> {
    connection
        .query_row(
            "SELECT
                COALESCE(SUM(CASE WHEN parser_id = 'timeline.macb' THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN parser_id = 'timeline.macb' THEN 0 ELSE 1 END), 0)
             FROM timeline_events",
            [],
            |row| Ok((row.get::<_, u64>(0)?, row.get::<_, u64>(1)?)),
        )
        .map_err(|error| format!("Count Timeline phase events: {error}"))
}
