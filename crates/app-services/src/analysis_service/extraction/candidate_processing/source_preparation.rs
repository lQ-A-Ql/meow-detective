use super::*;
use crate::analysis_service::extraction::evtx::extract_evtx_candidate;
use crate::analysis_service::extraction::evtx_persistence::{
    persist_evtx_candidate_from_read_seek, EvtxPersistenceResult,
};
use crate::analysis_service::extraction::reader::{
    read_candidate_source_with_progress, CancellableProgressReader,
};
use crate::analysis_service::extraction::state::PersistedExtractionOutcome;

pub(super) fn prepare_source<F>(
    coordinator: &mut CandidateCoordinator<'_, '_, '_, F>,
    candidate: EvidenceCandidate,
    capability: AnalysisCapability,
) -> Result<PreparedWork<PreparedCandidate, CandidateCompletion>, AnalysisServiceError>
where
    F: FnMut(&EvidenceCandidate, usize) -> Result<CandidateSource, String>,
{
    let read_limit = candidate_read_limit(&candidate, capability);
    if candidate.category == "EventLogs" {
        return prepare_evtx_source(coordinator, candidate, capability, read_limit);
    }

    let bytes = match read_candidate_bytes(coordinator, &candidate, capability, read_limit) {
        Ok(bytes) => bytes,
        Err(CandidateExtractionError::Warning(warning)) => {
            return Ok(ready_warning(candidate, capability, warning));
        }
        Err(CandidateExtractionError::Cancelled) => {
            return Err(AnalysisServiceError::Cancelled);
        }
    };
    let weight_bytes = bytes.len();
    Ok(PreparedWork::Parallel {
        input: PreparedCandidate {
            candidate,
            capability,
            input: PreparedCandidateInput::Bytes(bytes),
        },
        weight_bytes,
    })
}

fn prepare_evtx_source<F>(
    coordinator: &mut CandidateCoordinator<'_, '_, '_, F>,
    candidate: EvidenceCandidate,
    capability: AnalysisCapability,
    read_limit: usize,
) -> Result<PreparedWork<PreparedCandidate, CandidateCompletion>, AnalysisServiceError>
where
    F: FnMut(&EvidenceCandidate, usize) -> Result<CandidateSource, String>,
{
    let source = match (coordinator.file_reader)(&candidate, read_limit) {
        Ok(source) => source,
        Err(error) => {
            let warning = format!("{} read failed: {error}", candidate.path);
            return Ok(ready_deferred(candidate, capability, warning));
        }
    };
    let source = match source {
        CandidateSource::Seekable(mut reader) => {
            let total = usize::try_from(candidate.size).unwrap_or(usize::MAX);
            let cancel_token = coordinator.context.cancel_token;
            let progress = &mut coordinator.progress;
            let mut report = |bytes_read: usize| {
                progress.report_read_progress(capability, &candidate, bytes_read.min(total), total);
            };
            let mut monitored =
                CancellableProgressReader::new(reader.as_mut(), cancel_token, &mut report);
            let result = persist_evtx_candidate_from_read_seek(
                coordinator.context.conn,
                coordinator.context.case_id,
                &candidate,
                capability,
                &mut monitored,
                cancel_token,
            )?;
            ensure_not_cancelled(coordinator.context.cancel_token)?;
            return Ok(match result {
                EvtxPersistenceResult::Persisted(outcome) => {
                    ready_persisted(candidate, capability, outcome)
                }
                EvtxPersistenceResult::Deferred(error) => {
                    ready_deferred(candidate, capability, error.to_string())
                }
            });
        }
        source => source,
    };

    let probe_limit = read_limit.saturating_add(1);
    let bytes = match read_candidate_source_with_progress(
        &candidate,
        source,
        probe_limit,
        coordinator.context.cancel_token,
        |bytes_read| {
            coordinator.progress.report_read_progress(
                capability,
                &candidate,
                bytes_read.min(read_limit),
                read_limit,
            );
        },
    ) {
        Ok(bytes) => bytes,
        Err(CandidateExtractionError::Warning(warning)) => {
            return Ok(ready_deferred(candidate, capability, warning));
        }
        Err(CandidateExtractionError::Cancelled) => {
            return Err(AnalysisServiceError::Cancelled);
        }
    };
    if bytes.len() > read_limit || candidate.size > read_limit as u64 {
        let warning = format!(
            "{} exceeds the {} byte buffered EVTX limit and the evidence reader is not seekable",
            candidate.path, read_limit
        );
        return Ok(ready_deferred(candidate, capability, warning));
    }
    match extract_evtx_candidate(&candidate, &bytes) {
        Ok(outcome) => Ok(ready_outcome(candidate, capability, outcome)),
        Err(artifacts_windows::evtx::EvtxBootError::Cancelled) => {
            Err(AnalysisServiceError::Cancelled)
        }
        Err(error) => Ok(ready_deferred(candidate, capability, error.to_string())),
    }
}

fn read_candidate_bytes<F>(
    coordinator: &mut CandidateCoordinator<'_, '_, '_, F>,
    candidate: &EvidenceCandidate,
    capability: AnalysisCapability,
    read_limit: usize,
) -> Result<Vec<u8>, CandidateExtractionError>
where
    F: FnMut(&EvidenceCandidate, usize) -> Result<CandidateSource, String>,
{
    read_candidate_bytes_with_progress(
        candidate,
        read_limit,
        coordinator.context.cancel_token,
        coordinator.file_reader,
        |bytes_read| {
            coordinator
                .progress
                .report_read_progress(capability, candidate, bytes_read, read_limit);
        },
    )
}

fn ready_outcome(
    candidate: EvidenceCandidate,
    capability: AnalysisCapability,
    outcome: ExtractionOutcome,
) -> PreparedWork<PreparedCandidate, CandidateCompletion> {
    PreparedWork::Ready(CandidateCompletion {
        candidate,
        capability,
        kind: CandidateCompletionKind::Outcome(outcome),
    })
}

fn ready_persisted(
    candidate: EvidenceCandidate,
    capability: AnalysisCapability,
    outcome: PersistedExtractionOutcome,
) -> PreparedWork<PreparedCandidate, CandidateCompletion> {
    PreparedWork::Ready(CandidateCompletion {
        candidate,
        capability,
        kind: CandidateCompletionKind::Persisted(outcome),
    })
}

fn ready_deferred(
    candidate: EvidenceCandidate,
    capability: AnalysisCapability,
    warning: String,
) -> PreparedWork<PreparedCandidate, CandidateCompletion> {
    PreparedWork::Ready(CandidateCompletion {
        candidate,
        capability,
        kind: CandidateCompletionKind::Deferred(warning),
    })
}

fn ready_warning(
    candidate: EvidenceCandidate,
    capability: AnalysisCapability,
    warning: String,
) -> PreparedWork<PreparedCandidate, CandidateCompletion> {
    PreparedWork::Ready(CandidateCompletion {
        candidate,
        capability,
        kind: CandidateCompletionKind::Warning(warning),
    })
}
