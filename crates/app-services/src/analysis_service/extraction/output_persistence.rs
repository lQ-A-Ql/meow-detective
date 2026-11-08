use super::native_output;
use super::state::{AnalysisOutputProducer, ExtractionState};
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
    let data_source_id = resolve_output_data_source_id(conn)?;
    let database_path: String = conn
        .query_row("PRAGMA database_list", [], |row| row.get(2))
        .unwrap_or_default();
    let case_root = std::path::Path::new(&database_path)
        .parent()
        .and_then(std::path::Path::parent)
        .and_then(std::path::Path::parent);
    let native_ready = case_root.is_some()
        && !database_path.is_empty()
        && persistence_sqlite::source_fjall::open_store(conn)?.is_some();
    let native_generation = native_ready
        .then(|| {
            native_output::stage(
                conn,
                case_root.expect("native source has a case root"),
                &data_source_id,
                &artifacts,
                &events,
                &replacements,
            )
        })
        .transpose()?;
    validate_artifact_source_attribution(&artifacts, &data_source_id)?;
    commit_outputs(
        conn,
        case_id,
        &data_source_id,
        native_generation,
        &artifacts,
        &events,
        &replacements,
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
    native_generation: Option<native_output::StagedGeneration>,
    artifacts: &[domain::Artifact],
    events: &[domain::TimelineEvent],
    replacements: &[super::state::AnalysisOutputReplacement],
    clean_scans: &[persistence_sqlite::repositories::analysis_scan_repo::CleanAnalysisCandidateScan],
    diagnostic_scans: &[persistence_sqlite::repositories::analysis_scan_repo::DiagnosticAnalysisCandidateScan],
    complete_scans: &[persistence_sqlite::repositories::analysis_scan_repo::CompleteAnalysisCandidateScan],
) -> Result<(), AnalysisServiceError> {
    if native_generation.is_none() {
        crate::import_pipeline::begin_analysis_output_projection(conn)
            .map_err(|error| AnalysisServiceError::Other(error.to_string()))?;
    }
    let transaction = conn.unchecked_transaction()?;
    let artifact_repo = ArtifactRepo::new(&transaction);
    let timeline_repo = TimelineRepo::new(&transaction);
    if let Some(generation) = native_generation.as_ref() {
        persistence_sqlite::source_fjall::bind_generation(&transaction, &generation.name)?;
    }
    if native_generation.is_some() {
        persist_native_fingerprints(&transaction, case_id, data_source_id, artifacts)?;
    }
    if native_generation.is_none() {
        replace_legacy_outputs(&artifact_repo, &timeline_repo, replacements)?;
    }
    if native_generation.is_none() {
        if !artifacts.is_empty() {
            artifact_repo.insert_batch_in_transaction(artifacts, case_id, data_source_id)?;
        }
        if !events.is_empty() {
            timeline_repo.insert_batch_with_case_in_transaction(events, case_id)?;
        }
    }
    AnalysisScanRepo::new(&transaction).insert_all_checkpoint_batch_in_transaction(
        clean_scans,
        diagnostic_scans,
        complete_scans,
    )?;
    let data_source_id = data_source_id.to_owned();
    let database_path: String = conn
        .query_row("PRAGMA database_list", [], |row| row.get(2))
        .unwrap_or_default();
    let case_root = std::path::Path::new(&database_path)
        .parent()
        .and_then(std::path::Path::parent)
        .and_then(std::path::Path::parent);
    if native_generation.is_none() {
        if let Some(case_root) = case_root {
            let fjall_root =
                crate::source_db::fjall_source_storage_root(case_root, &data_source_id)
                    .map_err(|error| AnalysisServiceError::Other(error.to_string()))?;
            if fjall_root.is_dir() {
                let case_connection =
                    crate::connection::open_existing_case_db(&case_root.join("app.db"))
                        .map_err(|error| AnalysisServiceError::Other(error.to_string()))?;
                crate::import_pipeline::mirror_analysis_outputs_before_sqlite_commit(
                    &case_connection,
                    case_root,
                    case_id,
                    &transaction,
                    &data_source_id,
                )
                .map_err(|error| AnalysisServiceError::Other(error.to_string()))?;
            }
        }
    }
    transaction.commit()?;
    if let Some(mut generation) = native_generation {
        generation.commit();
        return Ok(());
    }
    mirror_published_outputs(conn, case_id)?;
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

fn replace_legacy_outputs(
    artifact_repo: &ArtifactRepo<'_>,
    timeline_repo: &TimelineRepo<'_>,
    replacements: &[super::state::AnalysisOutputReplacement],
) -> Result<(), AnalysisServiceError> {
    for replacement in replacements {
        match &replacement.producer {
            AnalysisOutputProducer::Prefix(prefix) => {
                artifact_repo.delete_analysis_outputs_in_transaction(
                    &replacement.source_object_id,
                    prefix,
                )?;
                timeline_repo.delete_analysis_outputs_in_transaction(
                    &replacement.source_object_id,
                    prefix,
                )?;
            }
            AnalysisOutputProducer::ExtractorId(extractor_id) => {
                artifact_repo.delete_analysis_outputs_by_extractor_in_transaction(
                    &replacement.source_object_id,
                    extractor_id,
                )?;
                timeline_repo.delete_analysis_outputs_by_parser_in_transaction(
                    &replacement.source_object_id,
                    extractor_id,
                )?;
            }
        }
    }
    Ok(())
}

fn mirror_published_outputs(conn: &Connection, case_id: &str) -> Result<(), AnalysisServiceError> {
    let database_path: String = conn
        .query_row("PRAGMA database_list", [], |row| row.get(2))
        .unwrap_or_default();
    let Some(case_root) = std::path::Path::new(&database_path)
        .parent()
        .and_then(std::path::Path::parent)
        .and_then(std::path::Path::parent)
    else {
        return Ok(());
    };
    let source_id = resolve_output_data_source_id(conn)?;
    let fjall_root = crate::source_db::fjall_source_storage_root(case_root, &source_id)
        .map_err(|error| AnalysisServiceError::Other(error.to_string()))?;
    if !fjall_root.is_dir() {
        return Ok(());
    }
    let case_connection = match persistence_sqlite::open_existing(&case_root.join("app.db")) {
        Ok(connection) => connection,
        Err(_) => return Ok(()),
    };
    crate::import_pipeline::mirror_analysis_outputs_from_connections(
        &case_connection,
        case_root,
        case_id,
        conn,
        &source_id,
    )
    .map(|_| ())
    .map_err(|error| AnalysisServiceError::Other(error.to_string()))
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
