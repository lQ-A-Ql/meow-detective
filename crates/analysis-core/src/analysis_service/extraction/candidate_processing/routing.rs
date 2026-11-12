//! Candidate → capability routing helpers split out of
//! `candidate_processing.rs` to keep that module inside the size budget.

use super::super::linux_sections::{linux_artifact_section, LinuxArtifactSection};
use crate::analysis_service::candidates::{normalize_evidence_path, EvidenceCandidate};
use crate::analysis_service::capability::{AnalysisCapability, LINUX_UMBRELLA_KEY};

pub(crate) fn capability_for_candidate(
    selected: &[AnalysisCapability],
    candidate: &EvidenceCandidate,
) -> Option<AnalysisCapability> {
    if candidate.category == LINUX_UMBRELLA_KEY {
        let normalized = normalize_evidence_path(&candidate.path);
        let section = linux_artifact_section(&normalized);
        debug_assert!(LinuxArtifactSection::ALL.contains(&section));
        debug_assert_eq!(LinuxArtifactSection::from_key(section.key()), Some(section));
        return selected
            .iter()
            .find(|item| item.key == section.key())
            .copied();
    }
    selected
        .iter()
        .find(|item| item.candidate_category == candidate.category)
        .copied()
}

pub(crate) fn discovery_categories(selected: &[AnalysisCapability]) -> Vec<&str> {
    let mut categories = Vec::new();
    for capability in selected {
        if !categories.contains(&capability.candidate_category) {
            categories.push(capability.candidate_category);
        }
    }
    categories
}
