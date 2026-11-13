use super::super::registry::extract_snapshot_candidate;
use super::completion::persist_candidate_outcome;
use super::{
    AnalysisServiceError, CandidateCompletionKind, EvidenceCandidate, RegistryPreloadContext,
    StreamingGeneration,
};

pub(super) fn parse(
    candidate: &EvidenceCandidate,
    preload: &RegistryPreloadContext,
    output: &StreamingGeneration,
) -> CandidateCompletionKind {
    match preload.snapshot(candidate) {
        Some(snapshot) => persist_candidate_outcome(
            output,
            extract_snapshot_candidate(candidate, snapshot, preload.boot_key(candidate)),
            Vec::new(),
        ),
        None => match preload.failure_reason(candidate) {
            Some(reason) => CandidateCompletionKind::Warning(reason.to_owned()),
            None => CandidateCompletionKind::Failed(AnalysisServiceError::Other(
                "Registry candidate has no preparation result".to_owned(),
            )),
        },
    }
}
