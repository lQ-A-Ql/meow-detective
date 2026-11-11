use super::native_output;
use super::state::ExtractionState;
use crate::analysis_service::candidates::LINUX_CATALOG_METADATA_KEY;
use crate::analysis_service::error::AnalysisServiceError;
use persistence_sqlite::repositories::analysis_scan_repo::AnalysisScanRepo;
use persistence_sqlite::repositories::analysis_scan_repo::ANALYSIS_SCAN_REVISION_KEY;
use rusqlite::Connection;

pub(super) fn commit_analysis_control(
    conn: &Connection,
    state: &mut ExtractionState,
    mut native_generation: native_output::StagedGeneration,
) -> Result<(), AnalysisServiceError> {
    let clean_scans = std::mem::take(&mut state.clean_scans);
    let diagnostic_scans = std::mem::take(&mut state.diagnostic_scans);
    let complete_scans = std::mem::take(&mut state.complete_scans);
    let linux_catalog = state.linux_catalog.take();
    let transaction = conn.unchecked_transaction()?;
    persistence_sqlite::source_fjall::bind_generation(&transaction, &native_generation.name)?;
    AnalysisScanRepo::new(&transaction).insert_all_checkpoint_batch_in_transaction(
        &clean_scans,
        &diagnostic_scans,
        &complete_scans,
    )?;
    if let Some(catalog) = linux_catalog {
        let value = serde_json::to_string(&catalog).map_err(|error| {
            AnalysisServiceError::Other(format!("encode Linux analysis catalog: {error}"))
        })?;
        let metadata =
            persistence_sqlite::repositories::source_meta_repo::SourceMetaRepo::new(&transaction);
        metadata.write(LINUX_CATALOG_METADATA_KEY, &value)?;
        metadata.bump_revision(ANALYSIS_SCAN_REVISION_KEY)?;
    }
    transaction.commit()?;
    native_generation.commit();
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
