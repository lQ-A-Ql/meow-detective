use super::super::ExtractionOutcome;
use super::{eml, mbox, pst};
use crate::analysis_service::candidates::EvidenceCandidate;

pub(in crate::analysis_service::extraction) fn extract_email_candidate(
    candidate: &EvidenceCandidate,
    bytes: &[u8],
) -> ExtractionOutcome {
    let path_lower = candidate.path.to_lowercase();
    if path_lower.ends_with(".mbox") {
        mbox::extract_mbox_candidate(candidate, bytes)
    } else if path_lower.ends_with(".pst") || path_lower.ends_with(".ost") {
        pst::extract_pst_candidate(candidate, bytes)
    } else {
        eml::extract_eml_candidate(candidate, bytes)
    }
}
