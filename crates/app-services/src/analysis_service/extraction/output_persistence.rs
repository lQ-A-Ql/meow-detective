use super::native_output;
use super::state::ExtractionState;
use crate::analysis_service::error::AnalysisServiceError;
use persistence_sqlite::repositories::analysis_scan_repo::AnalysisScanRepo;
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
    let data_source_id = resolve_output_data_source_id(conn)?;
    validate_artifact_source_attribution(&artifacts, &data_source_id)?;
    let database_path: String = conn
        .query_row("PRAGMA database_list", [], |row| row.get(2))
        .unwrap_or_default();
    let case_root = std::path::Path::new(&database_path)
        .parent()
        .and_then(std::path::Path::parent)
        .and_then(std::path::Path::parent)
        .ok_or_else(|| {
            AnalysisServiceError::InvalidInput(
                "analysis output requires a file-backed case source database".into(),
            )
        })?;
    persistence_sqlite::source_fjall::open_store(conn)?.ok_or_else(|| {
        AnalysisServiceError::InvalidInput("source detail storage is missing".into())
    })?;
    let native_generation = native_output::stage(
        conn,
        case_root,
        &data_source_id,
        &artifacts,
        &events,
        &replacements,
    )?;
    commit_outputs(
        conn,
        case_id,
        &data_source_id,
        native_generation,
        &artifacts,
        &clean_scans,
        &diagnostic_scans,
        &complete_scans,
    )
}

#[allow(clippy::too_many_arguments)]
fn commit_outputs(
    conn: &Connection,
    case_id: &str,
    data_source_id: &str,
    native_generation: native_output::StagedGeneration,
    artifacts: &[domain::Artifact],
    clean_scans: &[persistence_sqlite::repositories::analysis_scan_repo::CleanAnalysisCandidateScan],
    diagnostic_scans: &[persistence_sqlite::repositories::analysis_scan_repo::DiagnosticAnalysisCandidateScan],
    complete_scans: &[persistence_sqlite::repositories::analysis_scan_repo::CompleteAnalysisCandidateScan],
) -> Result<(), AnalysisServiceError> {
    let transaction = conn.unchecked_transaction()?;
    persistence_sqlite::source_fjall::bind_generation(&transaction, &native_generation.name)?;
    persist_native_fingerprints(&transaction, case_id, data_source_id, artifacts)?;
    AnalysisScanRepo::new(&transaction).insert_all_checkpoint_batch_in_transaction(
        clean_scans,
        diagnostic_scans,
        complete_scans,
    )?;
    transaction.commit()?;
    let mut native_generation = native_generation;
    native_generation.commit();
    Ok(())
}

fn persist_native_fingerprints(
    conn: &rusqlite::Transaction<'_>,
    case_id: &str,
    source_id: &str,
    artifacts: &[domain::Artifact],
) -> Result<(), AnalysisServiceError> {
    let fingerprints =
        persistence_sqlite::repositories::fingerprint_repo::ForensicFingerprintRepo::new(conn);
    if fingerprints.is_available()? {
        for artifact in artifacts {
            fingerprints.upsert_in_transaction(&domain::ForensicFingerprint::for_artifact(
                artifact, case_id, source_id,
            ))?;
        }
    }
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
