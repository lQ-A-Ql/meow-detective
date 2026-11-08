use super::state::AnalysisOutputProducer;
use crate::analysis_service::error::AnalysisServiceError;
use domain::{Artifact, TimelineEvent};
use persistence_fjall::{ArtifactRecordStore, DatabaseConfig, SourceStore, TimelineRecordStore};
use rusqlite::Connection;
use std::path::Path;
use uuid::Uuid;

pub(super) struct StagedGeneration {
    pub(super) name: String,
    path: std::path::PathBuf,
    committed: bool,
}

impl StagedGeneration {
    pub(super) fn commit(&mut self) {
        self.committed = true;
    }
}

impl Drop for StagedGeneration {
    fn drop(&mut self) {
        if !self.committed {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }
}

pub(super) fn stage(
    conn: &Connection,
    case_root: &Path,
    source_id: &str,
    artifacts: &[Artifact],
    events: &[TimelineEvent],
    replacements: &[super::state::AnalysisOutputReplacement],
) -> Result<StagedGeneration, AnalysisServiceError> {
    let source = persistence_sqlite::source_fjall::open_store(conn)?
        .ok_or_else(|| AnalysisServiceError::Other("source generation is missing".into()))?;
    let source = (*source).clone();
    let manifest = persistence_fjall::verify_source_generation(&source, source_id)
        .map_err(|error| AnalysisServiceError::Other(error.to_string()))?;
    let attempt = format!("analysis-{}", Uuid::new_v4());
    let layout =
        persistence_fjall::SourceBuildLayout::for_case_source(case_root, source_id, &attempt)
            .map_err(|error| AnalysisServiceError::Other(error.to_string()))?;
    std::fs::create_dir_all(layout.build_path())?;
    let staged = source
        .clone_to(layout.build_path(), DatabaseConfig::default())
        .map_err(|error| AnalysisServiceError::Other(error.to_string()))?;
    replace_records(&staged, source_id, replacements, artifacts, events)?;
    staged
        .database()
        .persist(persistence_fjall::PersistMode::SyncAll)
        .map_err(|error| AnalysisServiceError::Other(error.to_string()))?;
    persistence_fjall::seal_source_generation(
        &staged,
        source_id,
        &attempt,
        &manifest.projection_batch_digest,
    )
    .map_err(|error| AnalysisServiceError::Other(error.to_string()))?;
    Ok(StagedGeneration {
        name: format!("source.fjall.build.{attempt}"),
        path: layout.build_path(),
        committed: false,
    })
}

fn replace_records(
    store: &SourceStore,
    source_id: &str,
    replacements: &[super::state::AnalysisOutputReplacement],
    artifacts: &[Artifact],
    events: &[TimelineEvent],
) -> Result<(), AnalysisServiceError> {
    let artifact_store = ArtifactRecordStore::from_source_store(store.clone());
    let timeline_store = TimelineRecordStore::from_source_store(store.clone());
    let snapshot = artifact_store.read_snapshot();
    let mut artifact_ids = Vec::new();
    let mut timeline_ids = Vec::new();
    let mut artifact_cursor = None;
    let mut timeline_cursor = None;
    loop {
        let page = artifact_store
            .list_by_source(
                &snapshot,
                source_id,
                artifact_cursor.as_deref(),
                persistence_fjall::ScanDirection::Forward,
                512,
            )
            .map_err(|error| AnalysisServiceError::Other(error.to_string()))?;
        for record in page.records {
            if replacements
                .iter()
                .any(|replacement| matches_artifact(replacement, &record, source_id))
            {
                artifact_ids.push(record.id);
            }
        }
        artifact_cursor = page.next_cursor;
        if artifact_cursor.is_none() {
            break;
        }
    }
    loop {
        let page = timeline_store
            .list_by_source(
                &snapshot,
                source_id,
                timeline_cursor.as_deref(),
                persistence_fjall::ScanDirection::Forward,
                512,
            )
            .map_err(|error| AnalysisServiceError::Other(error.to_string()))?;
        for record in page.records {
            if replacements
                .iter()
                .any(|replacement| matches_timeline(replacement, &record))
            {
                timeline_ids.push(record.id);
            }
        }
        timeline_cursor = page.next_cursor;
        if timeline_cursor.is_none() {
            break;
        }
    }
    artifact_store
        .delete_batch(
            source_id,
            &artifact_ids,
            persistence_fjall::PersistMode::Buffer,
        )
        .map_err(|error| AnalysisServiceError::Other(error.to_string()))?;
    timeline_store
        .delete_batch(
            source_id,
            &timeline_ids,
            persistence_fjall::PersistMode::Buffer,
        )
        .map_err(|error| AnalysisServiceError::Other(error.to_string()))?;
    let artifact_records = artifacts
        .iter()
        .map(|artifact| persistence_sqlite::source_fjall::artifact_record(artifact, source_id))
        .collect::<Vec<_>>();
    artifact_store
        .upsert_batch(&artifact_records, persistence_fjall::PersistMode::Buffer)
        .map_err(|error| AnalysisServiceError::Other(error.to_string()))?;
    let timeline_records = events
        .iter()
        .map(|event| persistence_sqlite::source_fjall::timeline_record(event, source_id))
        .collect::<Vec<_>>();
    timeline_store
        .upsert_batch(&timeline_records, persistence_fjall::PersistMode::Buffer)
        .map_err(|error| AnalysisServiceError::Other(error.to_string()))?;
    Ok(())
}

fn matches_artifact(
    replacement: &super::state::AnalysisOutputReplacement,
    record: &persistence_fjall::ArtifactRecord,
    source_id: &str,
) -> bool {
    if record.data_source_id != source_id
        || replacement.source_object_id != record.source_object_id.clone().unwrap_or_default()
    {
        return false;
    }
    let producer = record
        .attrs
        .get("extractorId")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    match &replacement.producer {
        AnalysisOutputProducer::Prefix(prefix) => producer.starts_with(prefix),
        AnalysisOutputProducer::ExtractorId(id) => producer == id,
    }
}

fn matches_timeline(
    replacement: &super::state::AnalysisOutputReplacement,
    record: &persistence_fjall::TimelineRecord,
) -> bool {
    if replacement.source_object_id != record.source_object_id {
        return false;
    }
    match &replacement.producer {
        AnalysisOutputProducer::Prefix(prefix) => record
            .parser_id
            .as_deref()
            .is_some_and(|id| id.starts_with(prefix)),
        AnalysisOutputProducer::ExtractorId(id) => record.parser_id.as_deref() == Some(id),
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/analysis_service/extraction/native_output.rs"]
mod tests;
