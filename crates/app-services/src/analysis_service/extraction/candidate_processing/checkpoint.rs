use super::{checkpoint_key, CandidateCompletionKind, CandidateProcessingContext};
use crate::analysis_service::candidates::EvidenceCandidate;
use crate::analysis_service::capability::AnalysisCapability;
use crate::analysis_service::error::AnalysisServiceError;
use crate::analysis_service::extraction::artifact_query::already_has_v1_artifacts;

pub(super) fn completion(
    context: &CandidateProcessingContext<'_>,
    candidate: &EvidenceCandidate,
    capability: AnalysisCapability,
) -> Result<Option<CandidateCompletionKind>, AnalysisServiceError> {
    let key = checkpoint_key(candidate, capability);
    if let Some(warnings) = context.checkpoints.diagnostic.get(&key) {
        return Ok(Some(CandidateCompletionKind::ReplayDiagnostic(
            warnings.clone(),
        )));
    }
    if context.checkpoints.clean.contains(&key) {
        return Ok(Some(CandidateCompletionKind::ReplayClean));
    }
    if let Some(scan) = context.checkpoints.complete.get(&key) {
        return Ok(Some(CandidateCompletionKind::ReplayComplete(scan.clone())));
    }
    if !context.checkpoints.storage_available && already_has_v1_artifacts(context.conn, candidate)?
    {
        return Ok(Some(CandidateCompletionKind::ExistingV1));
    }
    Ok(None)
}
