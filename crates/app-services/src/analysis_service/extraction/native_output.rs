use super::output_digest::OutputDigestAccumulator;
use super::state::AnalysisOutputProducer;
use super::state::PersistedExtractionOutcome;
use crate::analysis_service::error::AnalysisServiceError;
use domain::{Artifact, TimelineEvent};
use persistence_source::fjall::{
    ArtifactRecordStore, DatabaseConfig, PersistMode, SourceStore, TimelineRecordStore,
};
use rusqlite::Connection;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use uuid::Uuid;

pub(crate) struct StagedGeneration {
    pub(crate) name: String,
    path: PathBuf,
    committed: bool,
}

impl StagedGeneration {
    pub(crate) fn commit(&mut self) {
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

pub(super) struct StreamingGeneration {
    staged: StagedGeneration,
    store: SourceStore,
    source_id: String,
    projection_batch_digest: String,
    wrote_artifacts: AtomicBool,
    wrote_events: AtomicBool,
}

pub(super) struct ConcurrentOutputBatch<'a> {
    generation: &'a StreamingGeneration,
    artifacts: Vec<Artifact>,
    events: Vec<TimelineEvent>,
    artifact_count: u64,
    timeline_event_count: u64,
    digest: OutputDigestAccumulator,
}

impl<'a> ConcurrentOutputBatch<'a> {
    pub(super) fn new(generation: &'a StreamingGeneration) -> Self {
        Self {
            generation,
            artifacts: Vec::new(),
            events: Vec::new(),
            artifact_count: 0,
            timeline_event_count: 0,
            digest: OutputDigestAccumulator::default(),
        }
    }

    pub(super) fn push(&mut self, mut outcome: super::ExtractionOutcome) {
        for artifact in outcome.artifacts {
            self.digest.record_artifact(&artifact);
            self.artifacts.push(artifact);
            self.artifact_count = self.artifact_count.saturating_add(1);
        }
        crate::timeline_service::retain_analysis_events(&mut outcome.timeline_events);
        for event in outcome.timeline_events {
            self.digest.record_timeline_event(&event);
            self.events.push(event);
            self.timeline_event_count = self.timeline_event_count.saturating_add(1);
        }
    }

    pub(super) fn pending_artifact_count(&self) -> usize {
        self.artifacts.len()
    }

    pub(super) fn flush(&mut self) -> Result<(), AnalysisServiceError> {
        if !self.artifacts.is_empty() {
            super::output_persistence::validate_artifact_source_attribution(
                &self.artifacts,
                &self.generation.source_id,
            )?;
            self.generation.write_batch(&self.artifacts, &[])?;
            self.artifacts.clear();
        }
        if !self.events.is_empty() {
            self.generation.write_batch(&[], &self.events)?;
            self.events.clear();
        }
        Ok(())
    }

    pub(super) fn finish(
        mut self,
        warnings: Vec<String>,
    ) -> Result<PersistedExtractionOutcome, AnalysisServiceError> {
        self.flush()?;
        Ok(PersistedExtractionOutcome {
            artifact_count: self.artifact_count,
            timeline_event_count: self.timeline_event_count,
            warnings,
            output_digest: self.digest.finish(),
        })
    }
}

impl StreamingGeneration {
    pub(super) fn parser_workspace(&self) -> Result<PathBuf, AnalysisServiceError> {
        self.store
            .database()
            .path()
            .parent()
            .map(|parent| parent.join("parser-inputs/browser"))
            .ok_or_else(|| {
                AnalysisServiceError::Other(
                    "analysis generation has no parser workspace parent".into(),
                )
            })
    }

    pub(super) fn write_batch(
        &self,
        artifacts: &[Artifact],
        events: &[TimelineEvent],
    ) -> Result<(), AnalysisServiceError> {
        if !artifacts.is_empty() {
            let records = artifacts
                .iter()
                .map(|artifact| {
                    persistence_source::detail::artifact_record(artifact, &self.source_id)
                })
                .collect::<Vec<_>>();
            ArtifactRecordStore::from_source_store(self.store.clone())
                .upsert_batch_independent(&records, PersistMode::Buffer)
                .map_err(|error| AnalysisServiceError::Other(error.to_string()))?;
            self.wrote_artifacts.store(true, Ordering::Relaxed);
        }
        if !events.is_empty() {
            let records = events
                .iter()
                .map(|event| persistence_source::detail::timeline_record(event, &self.source_id))
                .collect::<Vec<_>>();
            TimelineRecordStore::from_source_store(self.store.clone())
                .upsert_batch_independent(&records, PersistMode::Buffer)
                .map_err(|error| AnalysisServiceError::Other(error.to_string()))?;
            self.wrote_events.store(true, Ordering::Relaxed);
        }
        Ok(())
    }

    pub(super) fn persist_outcome(
        &self,
        outcome: super::ExtractionOutcome,
    ) -> Result<PersistedExtractionOutcome, AnalysisServiceError> {
        let warnings = outcome.warnings.clone();
        let mut batch = ConcurrentOutputBatch::new(self);
        batch.push(outcome);
        batch.finish(warnings)
    }

    pub(super) fn rebuild_derived_metrics(&self) -> Result<(), AnalysisServiceError> {
        // The clone carries the published generation's derived metrics, and
        // replacement deletes keep them exact, so only this run's record writes
        // require a rebuild.
        if self.wrote_artifacts.load(Ordering::Relaxed) {
            ArtifactRecordStore::from_source_store(self.store.clone())
                .rebuild_metrics_for_source(&self.source_id, PersistMode::Buffer)
                .map_err(|error| AnalysisServiceError::Other(error.to_string()))?;
        }
        if self.wrote_events.load(Ordering::Relaxed) {
            TimelineRecordStore::from_source_store(self.store.clone())
                .rebuild_count_indexes(&self.source_id, PersistMode::Buffer)
                .map_err(|error| AnalysisServiceError::Other(error.to_string()))?;
        }
        Ok(())
    }

    pub(super) fn seal(self) -> Result<StagedGeneration, AnalysisServiceError> {
        let Self {
            staged,
            store,
            source_id,
            projection_batch_digest,
            ..
        } = self;
        store
            .database()
            .persist(PersistMode::SyncAll)
            .map_err(|error| AnalysisServiceError::Other(error.to_string()))?;
        persistence_source::fjall::seal_source_generation(
            &store,
            &source_id,
            &staged.name,
            &projection_batch_digest,
        )
        .map_err(|error| AnalysisServiceError::Other(error.to_string()))?;
        drop(store);
        Ok(staged)
    }
}

pub(super) fn begin_streaming(
    conn: &Connection,
    source_id: &str,
    replacements: &[super::state::AnalysisOutputReplacement],
) -> Result<StreamingGeneration, AnalysisServiceError> {
    let owner = persistence_source::detail::source_id(conn)?
        .ok_or_else(|| AnalysisServiceError::InvalidInput("source identity is missing".into()))?;
    if owner != source_id {
        return Err(AnalysisServiceError::InvalidInput(format!(
            "analysis output source '{}' does not match source detail owner '{}'",
            source_id, owner
        )));
    }
    let source = persistence_source::detail::open_store(conn)?
        .ok_or_else(|| AnalysisServiceError::Other("source generation is missing".into()))?;
    let source = (*source).clone();
    let manifest = persistence_source::fjall::verify_source_generation(&source, source_id)
        .map_err(|error| AnalysisServiceError::Other(error.to_string()))?;
    let attempt = format!("analysis-{}", Uuid::new_v4());
    let layout = SourceCaseLayout::from_published_store(&source, source_id, &attempt)?;
    std::fs::create_dir_all(layout.build_path())?;
    let staged = source
        .clone_to(layout.build_path(), DatabaseConfig::default())
        .map_err(|error| AnalysisServiceError::Other(error.to_string()))?;
    delete_replacements(&staged, source_id, replacements)?;
    Ok(StreamingGeneration {
        staged: StagedGeneration {
            name: format!("source.fjall.build.{attempt}"),
            path: layout.build_path(),
            committed: false,
        },
        store: staged,
        source_id: source_id.to_owned(),
        projection_batch_digest: manifest.projection_batch_digest,
        wrote_artifacts: AtomicBool::new(false),
        wrote_events: AtomicBool::new(false),
    })
}

pub(super) fn stage(
    conn: &Connection,
    case_root: &Path,
    source_id: &str,
    artifacts: &[Artifact],
    events: &[TimelineEvent],
    replacements: &[super::state::AnalysisOutputReplacement],
) -> Result<StagedGeneration, AnalysisServiceError> {
    let source = persistence_source::detail::open_store(conn)?
        .ok_or_else(|| AnalysisServiceError::Other("source generation is missing".into()))?;
    let source = (*source).clone();
    let manifest = persistence_source::fjall::verify_source_generation(&source, source_id)
        .map_err(|error| AnalysisServiceError::Other(error.to_string()))?;
    let attempt = format!("analysis-{}", Uuid::new_v4());
    let layout = persistence_source::fjall::SourceBuildLayout::for_case_source(
        case_root, source_id, &attempt,
    )
    .map_err(|error| AnalysisServiceError::Other(error.to_string()))?;
    std::fs::create_dir_all(layout.build_path())?;
    let staged = source
        .clone_to(layout.build_path(), DatabaseConfig::default())
        .map_err(|error| AnalysisServiceError::Other(error.to_string()))?;
    replace_records(&staged, source_id, replacements, artifacts, events)?;
    staged
        .database()
        .persist(persistence_source::fjall::PersistMode::SyncAll)
        .map_err(|error| AnalysisServiceError::Other(error.to_string()))?;
    persistence_source::fjall::seal_source_generation(
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
    delete_replacements(store, source_id, replacements)?;
    let artifact_records = artifacts
        .iter()
        .map(|artifact| persistence_source::detail::artifact_record(artifact, source_id))
        .collect::<Vec<_>>();
    ArtifactRecordStore::from_source_store(store.clone())
        .upsert_batch(
            &artifact_records,
            persistence_source::fjall::PersistMode::Buffer,
        )
        .map_err(|error| AnalysisServiceError::Other(error.to_string()))?;
    let timeline_records = events
        .iter()
        .map(|event| persistence_source::detail::timeline_record(event, source_id))
        .collect::<Vec<_>>();
    TimelineRecordStore::from_source_store(store.clone())
        .upsert_batch(
            &timeline_records,
            persistence_source::fjall::PersistMode::Buffer,
        )
        .map_err(|error| AnalysisServiceError::Other(error.to_string()))?;
    Ok(())
}

fn delete_replacements(
    store: &SourceStore,
    source_id: &str,
    replacements: &[super::state::AnalysisOutputReplacement],
) -> Result<(), AnalysisServiceError> {
    if replacements.is_empty() {
        return Ok(());
    }
    let artifact_store = ArtifactRecordStore::from_source_store(store.clone());
    let timeline_store = TimelineRecordStore::from_source_store(store.clone());
    let artifacts = artifact_store.read_snapshot();
    let mut cursor = None;
    loop {
        let page = artifact_store
            .list_by_source(
                &artifacts,
                source_id,
                cursor.as_deref(),
                persistence_source::fjall::ScanDirection::Forward,
                512,
            )
            .map_err(|error| AnalysisServiceError::Other(error.to_string()))?;
        let ids = page
            .records
            .into_iter()
            .filter(|record| {
                replacements
                    .iter()
                    .any(|replacement| matches_artifact(replacement, record, source_id))
            })
            .map(|record| record.id)
            .collect::<Vec<_>>();
        artifact_store
            .delete_batch(source_id, &ids, PersistMode::Buffer)
            .map_err(|error| AnalysisServiceError::Other(error.to_string()))?;
        cursor = page.next_cursor;
        if cursor.is_none() {
            break;
        }
    }
    let timeline = timeline_store.read_snapshot();
    let mut cursor = None;
    loop {
        let page = timeline_store
            .list_by_source(
                &timeline,
                source_id,
                cursor.as_deref(),
                persistence_source::fjall::ScanDirection::Forward,
                512,
            )
            .map_err(|error| AnalysisServiceError::Other(error.to_string()))?;
        let ids = page
            .records
            .into_iter()
            .filter(|record| {
                replacements
                    .iter()
                    .any(|replacement| matches_timeline(replacement, record))
            })
            .map(|record| record.id)
            .collect::<Vec<_>>();
        timeline_store
            .delete_batch(source_id, &ids, PersistMode::Buffer)
            .map_err(|error| AnalysisServiceError::Other(error.to_string()))?;
        cursor = page.next_cursor;
        if cursor.is_none() {
            break;
        }
    }
    Ok(())
}

struct SourceCaseLayout;

impl SourceCaseLayout {
    fn from_published_store(
        source: &SourceStore,
        source_id: &str,
        attempt: &str,
    ) -> Result<persistence_source::fjall::SourceBuildLayout, AnalysisServiceError> {
        let source_dir = source.database().path().parent().ok_or_else(|| {
            AnalysisServiceError::Other("source generation has no source directory".into())
        })?;
        let case_root = source_dir
            .parent()
            .and_then(Path::parent)
            .ok_or_else(|| AnalysisServiceError::Other("source has no case root".into()))?;
        persistence_source::fjall::SourceBuildLayout::for_case_source(case_root, source_id, attempt)
            .map_err(|error| AnalysisServiceError::Other(error.to_string()))
    }
}

fn matches_artifact(
    replacement: &super::state::AnalysisOutputReplacement,
    record: &persistence_source::fjall::ArtifactRecord,
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
    record: &persistence_source::fjall::TimelineRecord,
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
