use std::{
    path::Path,
    sync::{atomic::AtomicBool, Arc},
};

use domain::{CaseId, DataSourceId, DataSourcePlatform};
use persistence_sqlite::repositories::processing_phase_repo::ProcessingPhase;
use serde_json::json;

use super::{
    outcome::DerivedFinalizationReport, phase_execution::run_cancellable_phase,
    phase_runner::ProcessingPhaseRunner,
};
use crate::{
    import_analysis::{self, SearchIndexPhaseOptions},
    source_db, timeline_service,
};

const MAX_PERSISTED_WARNING_DETAILS: usize = 100;

pub(super) struct SearchPhaseContext<'a> {
    pub(super) case_conn: &'a rusqlite::Connection,
    pub(super) case_root: &'a Path,
    pub(super) case_id: &'a CaseId,
    pub(super) data_source_id: &'a DataSourceId,
    pub(super) platform: DataSourcePlatform,
    pub(super) cancel_token: &'a Arc<AtomicBool>,
}

pub(super) fn run_timeline_phase(
    runner: &ProcessingPhaseRunner<'_>,
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
    cancel_token: &Arc<AtomicBool>,
    report: &mut DerivedFinalizationReport,
) {
    let timeline_identity = runner.input_fingerprint(ProcessingPhase::Timeline);
    run_cancellable_phase(
        runner,
        ProcessingPhase::Timeline,
        report,
        cancel_token,
        || {
            run_timeline_projection(
                case_conn,
                case_root,
                case_id,
                data_source_id,
                cancel_token,
                &timeline_identity,
            )
        },
    );
}

pub(super) fn run_search_phase(
    runner: &ProcessingPhaseRunner<'_>,
    context: SearchPhaseContext<'_>,
    report: &mut DerivedFinalizationReport,
) {
    run_cancellable_phase(
        runner,
        ProcessingPhase::Search,
        report,
        context.cancel_token,
        || run_search_projection(context),
    );
}

fn run_timeline_projection(
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
    cancel_token: &Arc<AtomicBool>,
    timeline_identity: &str,
) -> Result<String, String> {
    let source = source_db::open_ready_source_by_id(case_conn, case_root, case_id, data_source_id)
        .map_err(|error| format!("Open registered source database for Timeline phase: {error}"))?;
    let connection = source.connection;
    let projection = timeline_service::ensure_macb_timeline_projected_with_cancel_and_identity(
        &connection,
        cancel_token,
        timeline_identity,
    )
    .map_err(|error| error.to_string())?;
    if !projection.graph_complete {
        return Err("Timeline graph projection remains incomplete".to_string());
    }
    let (macb_total_count, artifact_generated_event_count) = timeline_event_counts(&connection)?;
    source_db::checkpoint_source_db(&connection).map_err(|error| error.to_string())?;
    let warning_details = projection
        .warnings
        .iter()
        .take(MAX_PERSISTED_WARNING_DETAILS)
        .cloned()
        .collect::<Vec<_>>();

    Ok(json!({
        "macbInsertedCount": projection.inserted_count,
        "macbTotalCount": macb_total_count,
        "artifactGeneratedEventCount": artifact_generated_event_count,
        "alreadyProjected": projection.already_projected,
        "warningCount": projection.warnings.len(),
        "warningDetails": warning_details,
        "warningDetailsTruncated": projection.warnings.len() > MAX_PERSISTED_WARNING_DETAILS,
    })
    .to_string())
}

fn run_search_projection(context: SearchPhaseContext<'_>) -> Result<String, String> {
    let source = source_db::open_ready_source_by_id(
        context.case_conn,
        context.case_root,
        context.case_id,
        context.data_source_id,
    )
    .map_err(|error| format!("Resolve registered source database for Search phase: {error}"))?;
    drop(source.connection);
    let db_path = source_db::registered_source_db_path(
        context.case_conn,
        context.case_root,
        context.data_source_id,
    )
    .map_err(|error| format!("Resolve registered source DB path for Search phase: {error}"))?;
    let index_dir = source_db::registered_source_index_dir(
        context.case_conn,
        context.case_root,
        context.data_source_id,
    )
    .map_err(|error| format!("Resolve registered search index path: {error}"))?;
    let stats = import_analysis::run_search_index_phase(SearchIndexPhaseOptions {
        db_path,
        data_source_id: context.data_source_id.clone(),
        platform: context.platform,
        index_dir,
        cancel_token: context.cancel_token.clone(),
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
