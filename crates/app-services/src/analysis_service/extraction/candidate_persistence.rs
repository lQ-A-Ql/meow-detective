use super::{
    output_digest::OutputDigestAccumulator,
    output_persistence::validate_artifact_source_attribution, state::PersistedExtractionOutcome,
    ExtractionOutcome,
};
use crate::analysis_service::{
    candidates::EvidenceCandidate, capability::AnalysisCapability, error::AnalysisServiceError,
};
use persistence_sqlite::repositories::{
    analysis_scan_repo::{
        AnalysisScanRepo, CleanAnalysisCandidateScan, CompleteAnalysisCandidateScan,
        DiagnosticAnalysisCandidateScan,
    },
    artifact_repo::ArtifactRepo,
    timeline_repo::TimelineRepo,
};
use rusqlite::Connection;

pub(super) struct CandidateCommit {
    pub(super) observation: PersistedExtractionOutcome,
    output_digest: String,
}

#[derive(Default)]
pub(super) struct CandidateOutputBatch {
    artifacts: Vec<domain::Artifact>,
    timeline_events: Vec<domain::TimelineEvent>,
    artifact_count: u64,
    timeline_event_count: u64,
    digest: OutputDigestAccumulator,
}

impl CandidateOutputBatch {
    pub(super) fn push(&mut self, mut outcome: ExtractionOutcome) {
        for artifact in outcome.artifacts {
            self.digest.record_artifact(&artifact);
            self.artifacts.push(artifact);
            self.artifact_count = self.artifact_count.saturating_add(1);
        }
        crate::timeline_service::retain_analysis_events(&mut outcome.timeline_events);
        for event in outcome.timeline_events {
            self.digest.record_timeline_event(&event);
            self.timeline_events.push(event);
            self.timeline_event_count = self.timeline_event_count.saturating_add(1);
        }
    }

    pub(super) fn pending_artifact_count(&self) -> usize {
        self.artifacts.len()
    }

    pub(super) fn flush(
        &mut self,
        connection: &Connection,
        case_id: &str,
        source_id: &str,
    ) -> Result<(), AnalysisServiceError> {
        if !self.artifacts.is_empty() {
            validate_artifact_source_attribution(&self.artifacts, source_id)?;
            ArtifactRepo::new(connection).insert_batch_in_transaction(
                &self.artifacts,
                case_id,
                source_id,
            )?;
            self.artifacts.clear();
        }
        if !self.timeline_events.is_empty() {
            TimelineRepo::new(connection)
                .insert_batch_with_case_in_transaction(&self.timeline_events, case_id)?;
            self.timeline_events.clear();
        }
        Ok(())
    }

    pub(super) fn finish(self, warnings: Vec<String>) -> CandidateCommit {
        CandidateCommit {
            observation: PersistedExtractionOutcome {
                artifact_count: self.artifact_count,
                timeline_event_count: self.timeline_event_count,
                warnings,
            },
            output_digest: self.digest.finish(),
        }
    }
}

pub(super) fn delete_previous_outputs(
    connection: &Connection,
    candidate: &EvidenceCandidate,
    capability: AnalysisCapability,
) -> Result<(), AnalysisServiceError> {
    ArtifactRepo::new(connection).delete_analysis_outputs_in_transaction(
        &candidate.file_id.0,
        capability.producer_prefix(),
    )?;
    TimelineRepo::new(connection).delete_analysis_outputs_in_transaction(
        &candidate.file_id.0,
        capability.producer_prefix(),
    )?;
    Ok(())
}

pub(super) fn persist_checkpoint(
    connection: &Connection,
    candidate: &EvidenceCandidate,
    capability: AnalysisCapability,
    commit: &CandidateCommit,
) -> Result<(), AnalysisServiceError> {
    let repository = AnalysisScanRepo::new(connection);
    if commit.observation.artifact_count > 0 || commit.observation.timeline_event_count > 0 {
        let scan = CompleteAnalysisCandidateScan {
            source_object_id: candidate.file_id.0.clone(),
            capability_key: capability.key.to_string(),
            extractor_version: capability.extractor_version().to_string(),
            source_size: candidate.size,
            content_identity: candidate.content_identity.clone(),
            artifact_count: commit.observation.artifact_count,
            timeline_event_count: commit.observation.timeline_event_count,
            output_digest: commit.output_digest.clone(),
            warnings: commit.observation.warnings.clone(),
        };
        repository.insert_all_checkpoint_batch_in_transaction(&[], &[], &[scan])?;
    } else if commit.observation.warnings.is_empty() {
        let scan = CleanAnalysisCandidateScan {
            source_object_id: candidate.file_id.0.clone(),
            capability_key: capability.key.to_string(),
            extractor_version: capability.extractor_version().to_string(),
            source_size: candidate.size,
            content_identity: candidate.content_identity.clone(),
        };
        repository.insert_all_checkpoint_batch_in_transaction(&[scan], &[], &[])?;
    } else {
        let scan = DiagnosticAnalysisCandidateScan {
            source_object_id: candidate.file_id.0.clone(),
            capability_key: capability.key.to_string(),
            extractor_version: capability.extractor_version().to_string(),
            source_size: candidate.size,
            content_identity: candidate.content_identity.clone(),
            warnings: commit.observation.warnings.clone(),
        };
        repository.insert_all_checkpoint_batch_in_transaction(&[], &[scan], &[])?;
    }
    Ok(())
}
