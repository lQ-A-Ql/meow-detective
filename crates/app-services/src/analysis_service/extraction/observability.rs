use crate::analysis_service::candidates::{normalize_evidence_path, LinuxArtifactCatalogSummary};
use crate::analysis_service::extraction::linux::{
    linux_candidate_read_limit, linux_candidate_support,
};
use crate::analysis_service::extraction::linux_sections::LinuxCandidateSupport;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use transport::dto::AnalysisParseStatusDto;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(super) struct LinuxSummaryObservability {
    pub(super) candidate_count: u64,
    pub(super) coverage_ratio: f32,
    pub(super) truncated: bool,
    pub(super) warnings: Vec<String>,
    pub(super) coverage_notes: Vec<String>,
}

pub(super) fn linux_summary_observability(
    catalog: &LinuxArtifactCatalogSummary,
    total_count: u64,
    parsed_sources: &HashSet<String>,
) -> LinuxSummaryObservability {
    let candidate_count = catalog.candidates.len() as u64;
    let parsed_candidate_count = catalog
        .candidates
        .iter()
        .filter(|candidate| parsed_sources.contains(&candidate.file_id))
        .count() as u64;
    let coverage_ratio = if candidate_count == 0 {
        if total_count > 0 {
            1.0
        } else {
            0.0
        }
    } else {
        parsed_candidate_count as f32 / candidate_count as f32
    };
    let mut coverage_notes = Vec::new();
    if candidate_count > 0 && total_count == 0 {
        coverage_notes.push(format!(
            "Found {candidate_count} Linux artifact candidate(s), but no structured artifacts have been extracted yet. Run LinuxArtifacts extraction or review parser support warnings."
        ));
    } else if candidate_count > parsed_candidate_count && total_count > 0 {
        coverage_notes.push(format!(
            "Structured output coverage is {parsed_candidate_count} of {candidate_count} Linux artifact candidate source(s). Sources without structured output may be empty, unsupported, or handled by the generic text parser; this is a coverage summary, not live extraction progress."
        ));
    }

    let mut truncated = false;
    let mut warnings = Vec::new();
    let unsupported_count = catalog
        .candidates
        .iter()
        .filter(|candidate| {
            !candidate.path.is_empty()
                && matches!(
                    linux_candidate_support(&normalize_evidence_path(&candidate.path)),
                    LinuxCandidateSupport::Unsupported
                )
        })
        .count() as u64;
    let text_fallback_count = catalog
        .candidates
        .iter()
        .filter(|candidate| {
            !candidate.path.is_empty()
                && matches!(
                    linux_candidate_support(&normalize_evidence_path(&candidate.path)),
                    LinuxCandidateSupport::TextFallback
                )
        })
        .count() as u64;

    for candidate in &catalog.candidates {
        if candidate.path.is_empty() {
            continue;
        }
        let normalized_path = normalize_evidence_path(&candidate.path);
        let source_limit = linux_candidate_read_limit(&normalized_path);
        if candidate.size > source_limit as u64 {
            truncated = true;
            warnings.push(format!(
                "{} is {} bytes; Linux extraction reads only the first {} bytes (per-source cap)",
                candidate.path, candidate.size, source_limit
            ));
        }
    }
    if unsupported_count > 0 {
        coverage_notes.push(format!(
            "{unsupported_count} Linux candidate source(s) are detected for coverage but do not yet have a structured first-pass parser."
        ));
    }
    if text_fallback_count > 0 {
        coverage_notes.push(format!(
            "{text_fallback_count} Linux text log/source file(s) are covered with generic line-level extraction."
        ));
    }

    LinuxSummaryObservability {
        candidate_count,
        coverage_ratio,
        truncated,
        warnings,
        coverage_notes,
    }
}

pub(super) fn linux_summary_status(
    total_count: u64,
    candidate_count: u64,
) -> AnalysisParseStatusDto {
    if total_count > 0 {
        AnalysisParseStatusDto::Parsed
    } else if candidate_count > 0 {
        AnalysisParseStatusDto::CandidateFound
    } else {
        AnalysisParseStatusDto::NotFound
    }
}
