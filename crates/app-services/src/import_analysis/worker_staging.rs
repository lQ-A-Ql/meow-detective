use super::error::ImportAnalysisError;
use super::worker_model::WorkerStats;
use crate::staging;
use rusqlite::{params, Connection};

#[derive(Debug)]
pub(super) struct IndexDocRow {
    pub(super) file_id: String,
    pub(super) path: String,
    pub(super) text: String,
    pub(super) language: String,
    pub(super) truncated: bool,
}

pub(super) fn clear_analysis_worker_rows(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "DELETE FROM artifact_rows;
         DELETE FROM timeline_rows;
         DELETE FROM index_docs;",
    )
}

pub(super) fn persist_worker_stats(
    conn: &Connection,
    stats: &WorkerStats,
) -> Result<(), ImportAnalysisError> {
    staging::set_worker_meta(conn, "processed_count", &stats.processed_count.to_string())?;
    staging::set_worker_meta(conn, "artifact_count", &stats.artifact_count.to_string())?;
    staging::set_worker_meta(conn, "timeline_count", &stats.timeline_count.to_string())?;
    staging::set_worker_meta(conn, "indexed_count", &stats.indexed_count.to_string())?;
    staging::set_worker_meta(conn, "warning_count", &stats.warning_count.to_string())?;
    staging::set_worker_meta(conn, "skipped_count", &stats.skipped_count.to_string())?;
    staging::set_worker_meta(conn, "failed_count", &stats.failed_count.to_string())?;
    Ok(())
}

pub(super) fn flush_worker_rows(
    conn: &Connection,
    artifacts: &mut Vec<domain::Artifact>,
    timeline_events: &mut Vec<domain::TimelineEvent>,
    index_docs: &mut Vec<IndexDocRow>,
) -> Result<(), ImportAnalysisError> {
    if artifacts.is_empty() && timeline_events.is_empty() && index_docs.is_empty() {
        return Ok(());
    }
    let tx = conn.unchecked_transaction().map_err(|error| {
        ImportAnalysisError::Staging(format!("Begin worker staging tx: {error}"))
    })?;
    insert_artifacts(&tx, artifacts)?;
    insert_timeline_events(&tx, timeline_events)?;
    insert_index_docs(&tx, index_docs)?;
    tx.commit().map_err(|error| {
        ImportAnalysisError::Staging(format!("Commit worker staging tx: {error}"))
    })?;
    artifacts.clear();
    timeline_events.clear();
    index_docs.clear();
    Ok(())
}

fn insert_artifacts(
    conn: &Connection,
    artifacts: &[domain::Artifact],
) -> Result<(), ImportAnalysisError> {
    let mut statement = conn
        .prepare_cached(
            "INSERT OR IGNORE INTO artifact_rows
             (id, file_id, artifact_type, extractor_id, extractor_version, confidence, source_attribution, display_name, summary, data_json, source_path, created_at)
              VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        )
        .map_err(|error| {
            ImportAnalysisError::Staging(format!("Prepare artifact staging insert: {error}"))
        })?;
    for artifact in artifacts {
        statement
            .execute(params![
                artifact.id.0,
                artifact.source_object_id.as_ref().map(|id| &id.0),
                artifact.family,
                artifact.extractor_id,
                artifact.extractor_version,
                artifact.confidence,
                artifact.source_attribution,
                artifact.title,
                artifact.summary,
                serde_json::to_string(&artifact.attrs).unwrap_or_else(|_| "{}".to_string()),
                "",
                artifact.created_at.to_rfc3339(),
            ])
            .map_err(|error| {
                ImportAnalysisError::Staging(format!("Insert artifact staging row: {error}"))
            })?;
    }
    Ok(())
}

fn insert_timeline_events(
    conn: &Connection,
    timeline_events: &[domain::TimelineEvent],
) -> Result<(), ImportAnalysisError> {
    let mut statement = conn
        .prepare_cached(
            "INSERT OR IGNORE INTO timeline_rows
             (id, file_id, timestamp, event_type, parser_id, parser_version, confidence, source_attribution, title, description, data_json)
              VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        )
        .map_err(|error| {
            ImportAnalysisError::Staging(format!("Prepare timeline staging insert: {error}"))
        })?;
    for event in timeline_events {
        statement
            .execute(params![
                event.id.0,
                event.source_object_id,
                event.timestamp.to_rfc3339(),
                event.event_type,
                event.parser_id,
                event.parser_version,
                event.confidence,
                event.source_attribution,
                event.title,
                event.description,
                serde_json::to_string(&event.attrs).unwrap_or_else(|_| "{}".to_string()),
            ])
            .map_err(|error| {
                ImportAnalysisError::Staging(format!("Insert timeline staging row: {error}"))
            })?;
    }
    Ok(())
}

fn insert_index_docs(
    conn: &Connection,
    index_docs: &[IndexDocRow],
) -> Result<(), ImportAnalysisError> {
    let mut statement = conn
        .prepare_cached(
            "INSERT OR REPLACE INTO index_docs
             (file_id, path, text, language, truncated)
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )
        .map_err(|error| {
            ImportAnalysisError::Staging(format!("Prepare index staging insert: {error}"))
        })?;
    for doc in index_docs {
        statement
            .execute(params![
                doc.file_id,
                doc.path,
                doc.text,
                doc.language,
                doc.truncated as i32,
            ])
            .map_err(|error| {
                ImportAnalysisError::Staging(format!("Insert index staging row: {error}"))
            })?;
    }
    Ok(())
}
