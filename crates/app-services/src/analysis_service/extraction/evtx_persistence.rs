use super::candidate_persistence::{
    delete_previous_outputs, persist_checkpoint, CandidateCommit, CandidateOutputBatch,
};
use super::evtx::project_evtx_event;
use super::output_persistence::resolve_output_data_source_id;
use super::{state::PersistedExtractionOutcome, ExtractionOutcome};
use crate::analysis_service::{
    cancellation::ensure_not_cancelled, candidates::EvidenceCandidate,
    capability::AnalysisCapability, error::AnalysisServiceError,
};
use artifacts_windows::{evtx::EvtxBootError, EvtxVisitError};
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
    match build_evtx_commit(&context, reader) {
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
) -> Result<CandidateCommit, EvtxTransactionFailure> {
    delete_previous_outputs(context.conn, context.candidate, context.capability)
        .map_err(EvtxTransactionFailure::Service)?;
    let mut batch = CandidateOutputBatch::default();
    let visit = artifacts_windows::visit_structured_events_from_read_seek(
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
            Ok::<_, AnalysisServiceError>(())
        },
    );
    let summary = match visit {
        Ok(summary) => summary,
        Err(EvtxVisitError::Parser(error)) => return Err(EvtxTransactionFailure::Parser(error)),
        Err(EvtxVisitError::Sink(error)) => return Err(EvtxTransactionFailure::Service(error)),
    };
    ensure_not_cancelled(context.cancel_token).map_err(EvtxTransactionFailure::Service)?;
    batch
        .flush(context.conn, context.case_id, context.data_source_id)
        .map_err(EvtxTransactionFailure::Service)?;
    let commit = batch.finish(summary.warnings);
    persist_checkpoint(context.conn, context.candidate, context.capability, &commit)
        .map_err(EvtxTransactionFailure::Service)?;
    Ok(commit)
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
        candidate.file_id.0, candidate.data_source_id, data_source_id,
    )))
}

#[cfg(test)]
#[path = "../../../tests/unit/analysis_service/extraction/evtx_persistence.rs"]
mod tests;
