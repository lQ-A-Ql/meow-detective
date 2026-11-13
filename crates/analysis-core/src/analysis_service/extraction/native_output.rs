use super::output_digest::OutputDigestAccumulator;
use super::state::PersistedExtractionOutcome;
use crate::analysis_service::error::AnalysisServiceError;
use domain::{Artifact, TimelineEvent};
use persistence_source::fjall::{
    ArtifactRecordStore, DatabaseConfig, FjallStorageError, PersistMode, SourceCloneProgress,
    SourceStore, TimelineRecordStore,
};
use rusqlite::Connection;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use uuid::Uuid;
mod replacement;
use replacement::delete_replacements;

pub struct StagedGeneration {
    pub name: String,
    path: PathBuf,
    committed: bool,
}

impl StagedGeneration {
    pub fn commit(&mut self) {
        self.committed = true;
    }

    fn create(
        layout: &persistence_source::fjall::SourceBuildLayout,
    ) -> Result<Self, AnalysisServiceError> {
        let path = layout.build_path();
        let parent = path.parent().ok_or_else(|| {
            AnalysisServiceError::Other("analysis build directory has no parent".into())
        })?;
        std::fs::create_dir_all(parent)?;
        std::fs::create_dir(&path)?;
        Ok(Self {
            name: format!("source.fjall.build.{}", layout.attempt_id()),
            path,
            committed: false,
        })
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
    // Release the database before the staging guard removes its directory.
    store: SourceStore,
    staged: StagedGeneration,
    source_id: String,
    verified_clone: persistence_source::fjall::VerifiedGenerationClone,
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
        timeline::retain_analysis_events(&mut outcome.timeline_events);
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

    pub(super) fn rebuild_derived_metrics(
        &self,
        cancel: &AtomicBool,
        mut progress: impl FnMut(u64),
    ) -> Result<(), AnalysisServiceError> {
        crate::analysis_service::cancellation::ensure_not_cancelled(cancel)?;
        let artifacts = ArtifactRecordStore::from_source_store(self.store.clone());
        let mut processed_artifacts = 0u64;
        if self.wrote_artifacts.load(Ordering::Relaxed) {
            artifacts
                .rebuild_metrics_for_source_with_control(
                    &self.source_id,
                    PersistMode::Buffer,
                    cancel,
                    |count| {
                        processed_artifacts = count;
                        progress(count);
                    },
                )
                .map_err(clone_error)?;
        }
        if self.wrote_events.load(Ordering::Relaxed) {
            crate::analysis_service::cancellation::ensure_not_cancelled(cancel)?;
            TimelineRecordStore::from_source_store(self.store.clone())
                .rebuild_count_indexes_with_control(
                    &self.source_id,
                    PersistMode::Buffer,
                    cancel,
                    |count| progress(processed_artifacts.saturating_add(count)),
                )
                .map_err(clone_error)?;
        }
        crate::analysis_service::cancellation::ensure_not_cancelled(cancel)?;
        Ok(())
    }

    pub(super) fn seal(
        self,
        cancel: &AtomicBool,
        mut progress: impl FnMut(u64, u64),
    ) -> Result<StagedGeneration, AnalysisServiceError> {
        let Self {
            staged,
            store,
            verified_clone,
            ..
        } = self;
        crate::analysis_service::cancellation::ensure_not_cancelled(cancel)?;
        progress(0, 0);
        store
            .database()
            .persist(PersistMode::SyncAll)
            .map_err(|error| AnalysisServiceError::Other(error.to_string()))?;
        verified_clone
            .seal(&store, &staged.name, cancel)
            .map_err(clone_error)?;
        progress(0, 0);
        drop(store);
        Ok(staged)
    }
}

pub(super) fn begin_streaming(
    conn: &Connection,
    source_id: &str,
    replacements: &[super::state::AnalysisOutputReplacement],
    cancel_token: &AtomicBool,
    progress: impl FnMut(SourceCloneProgress),
    deletion_progress: impl FnMut(u64),
) -> Result<StreamingGeneration, AnalysisServiceError> {
    crate::analysis_service::cancellation::ensure_not_cancelled(cancel_token)?;
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
    let attempt = format!("analysis-{}", Uuid::new_v4());
    let layout = SourceCaseLayout::from_published_store(&source, source_id, &attempt)?;
    let staged = StagedGeneration::create(&layout)?;
    let (store, verified_clone) =
        persistence_source::fjall::clone_verified_generation_with_control(
            &source,
            source_id,
            &staged.path,
            DatabaseConfig::default(),
            cancel_token,
            progress,
        )
        .map_err(clone_error)?;
    crate::analysis_service::cancellation::ensure_not_cancelled(cancel_token)?;
    delete_replacements(
        &store,
        source_id,
        replacements,
        cancel_token,
        deletion_progress,
    )?;
    Ok(StreamingGeneration {
        staged,
        store,
        source_id: source_id.to_owned(),
        verified_clone,
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
    let attempt = format!("analysis-{}", Uuid::new_v4());
    let layout = persistence_source::fjall::SourceBuildLayout::for_case_source(
        case_root, source_id, &attempt,
    )
    .map_err(|error| AnalysisServiceError::Other(error.to_string()))?;
    let staged = StagedGeneration::create(&layout)?;
    let (store, verified_clone) =
        persistence_source::fjall::clone_verified_generation_with_control(
            &source,
            source_id,
            &staged.path,
            DatabaseConfig::default(),
            &AtomicBool::new(false),
            |_| {},
        )
        .map_err(clone_error)?;
    replace_records(&store, source_id, replacements, artifacts, events)?;
    store
        .database()
        .persist(persistence_source::fjall::PersistMode::SyncAll)
        .map_err(|error| AnalysisServiceError::Other(error.to_string()))?;
    verified_clone
        .seal(&store, &attempt, &AtomicBool::new(false))
        .map_err(clone_error)?;
    drop(store);
    Ok(staged)
}

pub(super) fn clone_error(error: FjallStorageError) -> AnalysisServiceError {
    match error {
        FjallStorageError::Cancelled => AnalysisServiceError::Cancelled,
        other => AnalysisServiceError::Other(other.to_string()),
    }
}

fn replace_records(
    store: &SourceStore,
    source_id: &str,
    replacements: &[super::state::AnalysisOutputReplacement],
    artifacts: &[Artifact],
    events: &[TimelineEvent],
) -> Result<(), AnalysisServiceError> {
    delete_replacements(
        store,
        source_id,
        replacements,
        &AtomicBool::new(false),
        |_| {},
    )?;
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

#[cfg(test)]
#[path = "../../../tests/unit/analysis_service/extraction/native_output.rs"]
mod tests;
