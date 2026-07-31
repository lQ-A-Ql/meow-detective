use super::state::ExtractionState;
use crate::analysis_service::error::AnalysisServiceError;
use persistence_sqlite::repositories::{
    analysis_scan_repo::AnalysisScanRepo, artifact_repo::ArtifactRepo, timeline_repo::TimelineRepo,
};
use rusqlite::Connection;
use std::time::Instant;

pub(super) fn flush_pending_outputs(
    conn: &Connection,
    case_id: &str,
    state: &mut ExtractionState,
) -> Result<u64, AnalysisServiceError> {
    if !state.has_pending_outputs() {
        return Ok(0);
    }
    let started = Instant::now();
    persist_outputs(conn, case_id, state)?;
    Ok(u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX))
}

pub(super) fn persist_outputs(
    conn: &Connection,
    case_id: &str,
    state: &mut ExtractionState,
) -> Result<(), AnalysisServiceError> {
    let artifacts = std::mem::take(&mut state.artifacts);
    let mut events = std::mem::take(&mut state.events);
    crate::timeline_service::retain_analysis_events(&mut events);
    let replacements = std::mem::take(&mut state.replacements);
    let clean_scans = std::mem::take(&mut state.clean_scans);
    let diagnostic_scans = std::mem::take(&mut state.diagnostic_scans);
    let complete_scans = std::mem::take(&mut state.complete_scans);
    let transaction = conn.unchecked_transaction()?;

    let artifact_repo = ArtifactRepo::new(&transaction);
    let timeline_repo = TimelineRepo::new(&transaction);
    for replacement in replacements {
        artifact_repo.delete_analysis_outputs_in_transaction(
            &replacement.source_object_id,
            replacement.producer_prefix,
        )?;
        timeline_repo.delete_analysis_outputs_in_transaction(
            &replacement.source_object_id,
            replacement.producer_prefix,
        )?;
    }
    if !artifacts.is_empty() {
        let data_source_id = resolve_output_data_source_id(&transaction)?;
        validate_artifact_source_attribution(&artifacts, &data_source_id)?;
        artifact_repo.insert_batch_in_transaction(&artifacts, case_id, &data_source_id)?;
    }
    if !events.is_empty() {
        timeline_repo.insert_batch_with_case_in_transaction(&events, case_id)?;
    }
    AnalysisScanRepo::new(&transaction).insert_all_checkpoint_batch_in_transaction(
        &clean_scans,
        &diagnostic_scans,
        &complete_scans,
    )?;
    transaction.commit()?;
    Ok(())
}

pub(super) fn resolve_output_data_source_id(
    conn: &Connection,
) -> Result<String, AnalysisServiceError> {
    let mut statement = conn.prepare("SELECT id FROM data_sources ORDER BY id LIMIT 2")?;
    let source_ids = statement
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    match source_ids.as_slice() {
        [data_source_id] => Ok(data_source_id.clone()),
        [] => Err(AnalysisServiceError::InvalidInput(
            "analysis output source database has no registered data source".to_string(),
        )),
        _ => Err(AnalysisServiceError::InvalidInput(
            "analysis output source database contains multiple data sources".to_string(),
        )),
    }
}

pub(super) fn validate_artifact_source_attribution(
    artifacts: &[domain::Artifact],
    data_source_id: &str,
) -> Result<(), AnalysisServiceError> {
    for artifact in artifacts {
        let Some(attributed_source) = artifact
            .attrs
            .get("dataSourceId")
            .and_then(serde_json::Value::as_str)
        else {
            continue;
        };
        if attributed_source != data_source_id {
            return Err(AnalysisServiceError::InvalidInput(format!(
                "artifact '{}' attributes data source '{}', but the source database owns '{}'",
                artifact.id.0, attributed_source, data_source_id
            )));
        }
    }
    Ok(())
}
