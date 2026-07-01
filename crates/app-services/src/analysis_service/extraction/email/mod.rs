//! Email evidence extraction (EML/EMLX, mbox, PST/OST).
//!
//! This module was split from a single ~1800 line file into focused submodules:
//!
//! - `eml`: single-message EML/EMLX parsing (`parse_email_message`) and extraction
//! - `mbox`: mbox container extraction
//! - `pst`: PST/OST container extraction
//! - `shared`: helpers used by more than one of the above (body preview truncation)

mod eml;
mod mbox;
mod pst;
mod shared;

#[cfg(test)]
mod tests;

use super::ExtractionOutcome;
use crate::analysis_service::candidates::EvidenceCandidate;

pub(super) fn extract_email_candidate(
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
