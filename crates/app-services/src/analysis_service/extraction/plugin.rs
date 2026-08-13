//! Plugin candidate extraction (design doc §2.2).
//!
//! A plugin candidate runs through every loaded plugin whose declared
//! patterns match its path. Calls are serialized per plugin and wrapped in
//! `catch_unwind` inside `PluginExtractor` (M2); a failing plugin degrades to
//! a run warning plus an auditable failure record and never aborts the run.

use super::{ExtractionOutcome, PluginExtractFailure};
use crate::analysis_service::candidates::{normalize_evidence_path, EvidenceCandidate};
use artifacts_core::{ArtifactContext, ArtifactExtractor, VecSink};

pub(super) struct PluginCandidateOutcome {
    pub(super) outcome: ExtractionOutcome,
    pub(super) failures: Vec<PluginExtractFailure>,
}

/// Run every plugin supporting the candidate path over the bounded candidate
/// bytes and merge their payloads into one outcome. Provenance fields
/// (`extractor_id` = plugin id, versions, source attribution) are enforced by
/// `PluginExtractor`, never by the plugin.
pub(super) fn extract_plugin_candidate(
    candidate: &EvidenceCandidate,
    bytes: &[u8],
    plugins: &[&dyn ArtifactExtractor],
) -> PluginCandidateOutcome {
    let normalized = normalize_evidence_path(&candidate.path);
    let mut outcome = ExtractionOutcome::default();
    let mut failures = Vec::new();
    for plugin in plugins
        .iter()
        .filter(|plugin| plugin.supports_path(&normalized))
    {
        let ctx = ArtifactContext {
            file_id: candidate.file_id.clone(),
            file_path: candidate.path.clone(),
            reader: Box::new(std::io::Cursor::new(bytes.to_vec())),
        };
        let mut sink = VecSink::new();
        match plugin.run(ctx, &mut sink) {
            Ok(report) => {
                outcome.artifacts.extend(sink.artifacts);
                outcome.timeline_events.extend(sink.timeline_events);
                outcome.warnings.extend(report.errors);
            }
            Err(error) => {
                outcome.warnings.push(format!(
                    "plugin {} failed to extract {}: {}",
                    plugin.id(),
                    candidate.path,
                    error
                ));
                failures.push(PluginExtractFailure {
                    plugin_id: plugin.id().to_string(),
                    source_path: candidate.path.clone(),
                    error,
                });
            }
        }
    }
    PluginCandidateOutcome { outcome, failures }
}

/// Extractor ids of the plugins that ran on a plugin candidate. Used for
/// exact-id output replacement on re-extraction (plugin outputs carry the
/// bare plugin id, so the shared prefix mechanism cannot address them).
pub(super) fn matching_producer_ids(
    plugins: &[&dyn ArtifactExtractor],
    candidate: &EvidenceCandidate,
) -> Vec<String> {
    let normalized = normalize_evidence_path(&candidate.path);
    plugins
        .iter()
        .filter(|plugin| plugin.supports_path(&normalized))
        .map(|plugin| plugin.id().to_string())
        .collect()
}

#[cfg(test)]
#[path = "../../../tests/unit/analysis_service/extraction/plugin.rs"]
mod tests;
