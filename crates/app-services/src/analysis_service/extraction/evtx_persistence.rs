use super::evtx::project_evtx_event;
use super::output_digest::OutputDigestAccumulator;
use super::output_persistence::{
    resolve_output_data_source_id, validate_artifact_source_attribution,
};
use super::state::PersistedExtractionOutcome;
use super::ExtractionOutcome;
use crate::analysis_service::cancellation::ensure_not_cancelled;
use crate::analysis_service::candidates::EvidenceCandidate;
use crate::analysis_service::capability::AnalysisCapability;
use crate::analysis_service::error::AnalysisServiceError;
use crate::analysis_service::ANALYSIS_EXTRACTOR_VERSION;
use artifacts_windows::evtx::EvtxBootError;
use artifacts_windows::EvtxVisitError;
use persistence_sqlite::repositories::{
    analysis_scan_repo::{
        AnalysisScanRepo, CleanAnalysisCandidateScan, CompleteAnalysisCandidateScan,
        DiagnosticAnalysisCandidateScan,
    },
    artifact_repo::ArtifactRepo,
    timeline_repo::TimelineRepo,
};
use rusqlite::Connection;
use std::sync::atomic::AtomicBool;

const EVTX_PERSIST_BATCH_SIZE: usize = 256;

pub(super) enum EvtxPersistenceResult {
    Persisted(PersistedExtractionOutcome),
    Deferred(EvtxBootError),
}

enum EvtxTransactionFailure {
    Parser(EvtxBootError),
    Service(AnalysisServiceError),
}

struct PersistedEvtxCommit {
    observation: PersistedExtractionOutcome,
    output_digest: String,
}

struct EvtxCommitContext<'a> {
    conn: &'a Connection,
    case_id: &'a str,
    data_source_id: &'a str,
    candidate: &'a EvidenceCandidate,
    capability: AnalysisCapability,
    cancel_token: &'a AtomicBool,
    batch_size: usize,
}

pub(super) fn persist_evtx_candidate_from_read_seek(
    conn: &Connection,
    case_id: &str,
    candidate: &EvidenceCandidate,
    capability: AnalysisCapability,
    reader: &mut dyn evidence_core::ReadSeek,
    cancel_token: &AtomicBool,
) -> Result<EvtxPersistenceResult, AnalysisServiceError> {
    persist_evtx_candidate_with_batch_size(
        conn,
        case_id,
        candidate,
        capability,
        reader,
        cancel_token,
        EVTX_PERSIST_BATCH_SIZE,
    )
}

fn persist_evtx_candidate_with_batch_size(
    conn: &Connection,
    case_id: &str,
    candidate: &EvidenceCandidate,
    capability: AnalysisCapability,
    reader: &mut dyn evidence_core::ReadSeek,
    cancel_token: &AtomicBool,
    batch_size: usize,
) -> Result<EvtxPersistenceResult, AnalysisServiceError> {
    ensure_not_cancelled(cancel_token)?;
    let data_source_id = resolve_output_data_source_id(conn)?;
    validate_candidate_source(candidate, &data_source_id)?;
    let transaction = conn.unchecked_transaction()?;
    let context = EvtxCommitContext {
        conn: &transaction,
        case_id,
        data_source_id: &data_source_id,
        candidate,
        capability,
        cancel_token,
        batch_size: batch_size.max(1),
    };
    let result = build_evtx_commit(&context, reader);
    match result {
        Ok(persisted) => {
            transaction.commit()?;
            Ok(EvtxPersistenceResult::Persisted(persisted.observation))
        }
        Err(failure) => {
            transaction.rollback()?;
            match failure {
                EvtxTransactionFailure::Parser(EvtxBootError::Cancelled) => {
                    Err(AnalysisServiceError::Cancelled)
                }
                EvtxTransactionFailure::Parser(EvtxBootError::SourceIo {
                    kind: std::io::ErrorKind::Interrupted,
                    ..
                }) if cancel_token.load(std::sync::atomic::Ordering::Relaxed) => {
                    Err(AnalysisServiceError::Cancelled)
                }
                EvtxTransactionFailure::Parser(error) => Ok(EvtxPersistenceResult::Deferred(error)),
                EvtxTransactionFailure::Service(error) => Err(error),
            }
        }
    }
}

fn build_evtx_commit(
    context: &EvtxCommitContext<'_>,
    reader: &mut dyn evidence_core::ReadSeek,
) -> Result<PersistedEvtxCommit, EvtxTransactionFailure> {
    delete_previous_outputs(context.conn, context.candidate, context.capability)
        .map_err(EvtxTransactionFailure::Service)?;
    let mut batch = EvtxOutputBatch::default();
    let visit_result = artifacts_windows::visit_structured_events_from_read_seek(
        reader,
        &context.candidate.path,
        |event| {
            ensure_not_cancelled(context.cancel_token)?;
            let mut outcome = ExtractionOutcome::default();
            project_evtx_event(context.candidate, &event, &mut outcome);
            batch.push(outcome);
            if batch.pending_artifact_count() >= context.batch_size {
                batch.flush(context.conn, context.case_id, context.data_source_id)?;
            }
            Ok::<(), AnalysisServiceError>(())
        },
    );
    let summary = match visit_result {
        Ok(summary) => summary,
        Err(EvtxVisitError::Parser(error)) => {
            return Err(EvtxTransactionFailure::Parser(error));
        }
        Err(EvtxVisitError::Sink(error)) => {
            return Err(EvtxTransactionFailure::Service(error));
        }
    };
    ensure_not_cancelled(context.cancel_token).map_err(EvtxTransactionFailure::Service)?;
    batch
        .flush(context.conn, context.case_id, context.data_source_id)
        .map_err(EvtxTransactionFailure::Service)?;
    let persisted = batch.finish(summary.warnings);
    persist_checkpoint(
        context.conn,
        context.candidate,
        context.capability,
        &persisted,
    )
    .map_err(EvtxTransactionFailure::Service)?;
    Ok(persisted)
}

#[derive(Default)]
struct EvtxOutputBatch {
    artifacts: Vec<domain::Artifact>,
    timeline_events: Vec<domain::TimelineEvent>,
    artifact_count: u64,
    timeline_event_count: u64,
    digest: OutputDigestAccumulator,
}

impl EvtxOutputBatch {
    fn push(&mut self, outcome: ExtractionOutcome) {
        for artifact in outcome.artifacts {
            self.digest.record_artifact(&artifact);
            self.artifacts.push(artifact);
            self.artifact_count = self.artifact_count.saturating_add(1);
        }
        for event in outcome.timeline_events {
            self.digest.record_timeline_event(&event);
            self.timeline_events.push(event);
            self.timeline_event_count = self.timeline_event_count.saturating_add(1);
        }
    }

    fn pending_artifact_count(&self) -> usize {
        self.artifacts.len()
    }

    fn flush(
        &mut self,
        conn: &Connection,
        case_id: &str,
        data_source_id: &str,
    ) -> Result<(), AnalysisServiceError> {
        if !self.artifacts.is_empty() {
            validate_artifact_source_attribution(&self.artifacts, data_source_id)?;
            ArtifactRepo::new(conn).insert_batch_in_transaction(
                &self.artifacts,
                case_id,
                data_source_id,
            )?;
            self.artifacts.clear();
        }
        if !self.timeline_events.is_empty() {
            TimelineRepo::new(conn)
                .insert_batch_with_case_in_transaction(&self.timeline_events, case_id)?;
            self.timeline_events.clear();
        }
        Ok(())
    }

    fn finish(self, warnings: Vec<String>) -> PersistedEvtxCommit {
        PersistedEvtxCommit {
            observation: PersistedExtractionOutcome {
                artifact_count: self.artifact_count,
                timeline_event_count: self.timeline_event_count,
                warnings,
            },
            output_digest: self.digest.finish(),
        }
    }
}

impl PersistedEvtxCommit {
    fn has_outputs(&self) -> bool {
        self.observation.artifact_count > 0 || self.observation.timeline_event_count > 0
    }
}

fn delete_previous_outputs(
    conn: &Connection,
    candidate: &EvidenceCandidate,
    capability: AnalysisCapability,
) -> Result<(), AnalysisServiceError> {
    ArtifactRepo::new(conn).delete_analysis_outputs_in_transaction(
        &candidate.file_id.0,
        capability.producer_prefix(),
    )?;
    TimelineRepo::new(conn).delete_analysis_outputs_in_transaction(
        &candidate.file_id.0,
        capability.producer_prefix(),
    )?;
    Ok(())
}

fn persist_checkpoint(
    conn: &Connection,
    candidate: &EvidenceCandidate,
    capability: AnalysisCapability,
    outcome: &PersistedEvtxCommit,
) -> Result<(), AnalysisServiceError> {
    let repository = AnalysisScanRepo::new(conn);
    if outcome.has_outputs() {
        let scan = CompleteAnalysisCandidateScan {
            source_object_id: candidate.file_id.0.clone(),
            capability_key: capability.key.to_string(),
            extractor_version: ANALYSIS_EXTRACTOR_VERSION.to_string(),
            source_size: candidate.size,
            content_identity: candidate.content_identity.clone(),
            artifact_count: outcome.observation.artifact_count,
            timeline_event_count: outcome.observation.timeline_event_count,
            output_digest: outcome.output_digest.clone(),
            warnings: outcome.observation.warnings.clone(),
        };
        repository.insert_all_checkpoint_batch_in_transaction(&[], &[], &[scan])?;
    } else if outcome.observation.warnings.is_empty() {
        let scan = CleanAnalysisCandidateScan {
            source_object_id: candidate.file_id.0.clone(),
            capability_key: capability.key.to_string(),
            extractor_version: ANALYSIS_EXTRACTOR_VERSION.to_string(),
            source_size: candidate.size,
            content_identity: candidate.content_identity.clone(),
        };
        repository.insert_all_checkpoint_batch_in_transaction(&[scan], &[], &[])?;
    } else {
        let scan = DiagnosticAnalysisCandidateScan {
            source_object_id: candidate.file_id.0.clone(),
            capability_key: capability.key.to_string(),
            extractor_version: ANALYSIS_EXTRACTOR_VERSION.to_string(),
            source_size: candidate.size,
            content_identity: candidate.content_identity.clone(),
            warnings: outcome.observation.warnings.clone(),
        };
        repository.insert_all_checkpoint_batch_in_transaction(&[], &[scan], &[])?;
    }
    Ok(())
}

fn validate_candidate_source(
    candidate: &EvidenceCandidate,
    data_source_id: &str,
) -> Result<(), AnalysisServiceError> {
    if candidate.data_source_id == data_source_id {
        return Ok(());
    }
    Err(AnalysisServiceError::InvalidInput(format!(
        "EVTX candidate '{}' belongs to data source '{}', but the source database owns '{}'",
        candidate.file_id.0, candidate.data_source_id, data_source_id
    )))
}

#[cfg(test)]
#[path = "../../../tests/unit/analysis_service/extraction/evtx_persistence.rs"]
mod tests;
