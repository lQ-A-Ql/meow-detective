use std::collections::{BTreeMap, HashMap};

use persistence_sqlite::repositories::{
    analysis_scan_repo::{AnalysisScanRepo, CompleteAnalysisCandidateScan},
    artifact_repo::ArtifactRepo,
    timeline_repo::TimelineRepo,
};
use rusqlite::Connection;

use super::output_digest::OutputDigestAccumulator;
use crate::analysis_service::{
    capability::{find_capability, AnalysisCapability},
    error::AnalysisServiceError,
    ANALYSIS_EXTRACTOR_VERSION,
};

type CompleteScanKey = (String, String, u64, String);
const CHECKPOINT_VALIDATION_BATCH_SIZE: usize = 128;

pub(super) fn existing_complete_scan_keys(
    conn: &Connection,
) -> Result<HashMap<CompleteScanKey, CompleteAnalysisCandidateScan>, AnalysisServiceError> {
    let scans =
        AnalysisScanRepo::new(conn).list_complete_for_version(ANALYSIS_EXTRACTOR_VERSION)?;
    if scans.is_empty() {
        return Ok(HashMap::new());
    }

    let mut valid = HashMap::with_capacity(scans.len());
    for (capability_key, capability_scans) in scans_by_capability(scans) {
        let Some(capability) = find_capability(&capability_key) else {
            for scan in capability_scans {
                log_invalid_checkpoint(&scan);
            }
            continue;
        };
        for batch in capability_scans.chunks(CHECKPOINT_VALIDATION_BATCH_SIZE) {
            validate_checkpoint_batch(conn, capability, batch, &mut valid)?;
        }
    }
    Ok(valid)
}

fn scans_by_capability(
    scans: Vec<CompleteAnalysisCandidateScan>,
) -> BTreeMap<String, Vec<CompleteAnalysisCandidateScan>> {
    let mut grouped = BTreeMap::new();
    for scan in scans {
        grouped
            .entry(scan.capability_key.clone())
            .or_insert_with(Vec::new)
            .push(scan);
    }
    grouped
}

fn validate_checkpoint_batch(
    conn: &Connection,
    capability: AnalysisCapability,
    scans: &[CompleteAnalysisCandidateScan],
    valid: &mut HashMap<CompleteScanKey, CompleteAnalysisCandidateScan>,
) -> Result<(), AnalysisServiceError> {
    let source_ids = scans
        .iter()
        .map(|scan| scan.source_object_id.as_str())
        .collect::<Vec<_>>();
    let mut output_states = scans
        .iter()
        .map(|scan| {
            (
                scan.source_object_id.clone(),
                CheckpointOutputState::new(&scan.extractor_version),
            )
        })
        .collect::<HashMap<_, _>>();
    ArtifactRepo::new(conn).visit_analysis_outputs_for_sources(
        &source_ids,
        capability.producer_prefix(),
        |artifact| {
            let Some(source_object_id) = artifact.source_object_id.as_ref() else {
                return Ok(());
            };
            if let Some(state) = output_states.get_mut(&source_object_id.0) {
                state.record_artifact(&artifact, capability);
            }
            Ok(())
        },
    )?;
    TimelineRepo::new(conn).visit_analysis_outputs_for_sources(
        &source_ids,
        capability.producer_prefix(),
        |event| {
            if let Some(state) = output_states.get_mut(&event.source_object_id) {
                state.record_timeline_event(&event, capability);
            }
            Ok(())
        },
    )?;
    for scan in scans {
        let output_state = output_states
            .remove(&scan.source_object_id)
            .unwrap_or_else(|| CheckpointOutputState::new(&scan.extractor_version));
        if output_state.matches(scan) {
            valid.insert(
                (
                    scan.source_object_id.clone(),
                    scan.capability_key.clone(),
                    scan.source_size,
                    scan.content_identity.clone(),
                ),
                scan.clone(),
            );
        } else {
            log_invalid_checkpoint(scan);
        }
    }
    Ok(())
}

fn log_invalid_checkpoint(scan: &CompleteAnalysisCandidateScan) {
    tracing::warn!(
        source_object_id = %scan.source_object_id,
        capability = %scan.capability_key,
        "Ignoring analysis checkpoint whose persisted outputs no longer match"
    );
}

struct CheckpointOutputState {
    expected_version: String,
    artifact_count: u64,
    timeline_event_count: u64,
    artifact_versions_match: bool,
    timeline_versions_match: bool,
    digest: OutputDigestAccumulator,
}

impl CheckpointOutputState {
    fn new(expected_version: &str) -> Self {
        Self {
            expected_version: expected_version.to_owned(),
            artifact_count: 0,
            timeline_event_count: 0,
            artifact_versions_match: true,
            timeline_versions_match: true,
            digest: OutputDigestAccumulator::default(),
        }
    }

    fn record_artifact(&mut self, artifact: &domain::Artifact, capability: AnalysisCapability) {
        if !artifact_matches(artifact, capability) {
            return;
        }
        self.artifact_count = self.artifact_count.saturating_add(1);
        self.artifact_versions_match &=
            artifact.extractor_version.as_deref() == Some(self.expected_version.as_str());
        self.digest.record_artifact(artifact);
    }

    fn record_timeline_event(
        &mut self,
        event: &domain::TimelineEvent,
        capability: AnalysisCapability,
    ) {
        if !event_matches(event, capability) {
            return;
        }
        self.timeline_event_count = self.timeline_event_count.saturating_add(1);
        self.timeline_versions_match &=
            event.parser_version.as_deref() == Some(self.expected_version.as_str());
        self.digest.record_timeline_event(event);
    }

    fn matches(self, scan: &CompleteAnalysisCandidateScan) -> bool {
        self.artifact_count == scan.artifact_count
            && self.timeline_event_count == scan.timeline_event_count
            && self.artifact_versions_match
            && self.timeline_versions_match
            && self.digest.finish() == scan.output_digest
    }
}

fn artifact_matches(artifact: &domain::Artifact, capability: AnalysisCapability) -> bool {
    artifact
        .extractor_id
        .as_deref()
        .is_some_and(|producer| producer.starts_with(capability.producer_prefix()))
}

fn event_matches(event: &domain::TimelineEvent, capability: AnalysisCapability) -> bool {
    event
        .parser_id
        .as_deref()
        .is_some_and(|producer| producer.starts_with(capability.producer_prefix()))
}
