use std::collections::{BTreeMap, HashMap};

use persistence_sqlite::repositories::{
    analysis_scan_repo::{AnalysisScanRepo, CompleteAnalysisCandidateScan},
    artifact_repo::ArtifactRepo,
    timeline_repo::TimelineRepo,
};
use rusqlite::Connection;

use super::state::output_digest_for_outputs;
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
    let artifacts = group_artifacts_by_source(
        ArtifactRepo::new(conn)
            .list_analysis_outputs_for_sources(&source_ids, capability.producer_prefix())?,
    );
    let timeline_events = group_events_by_source(
        TimelineRepo::new(conn)
            .list_analysis_outputs_for_sources(&source_ids, capability.producer_prefix())?,
    );
    for scan in scans {
        if complete_scan_outputs_match(scan, capability, &artifacts, &timeline_events) {
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

fn group_artifacts_by_source(
    artifacts: Vec<domain::Artifact>,
) -> HashMap<String, Vec<domain::Artifact>> {
    let mut grouped = HashMap::new();
    for artifact in artifacts {
        let Some(source_object_id) = artifact.source_object_id.as_ref() else {
            continue;
        };
        grouped
            .entry(source_object_id.0.clone())
            .or_insert_with(Vec::new)
            .push(artifact);
    }
    grouped
}

fn group_events_by_source(
    events: Vec<domain::TimelineEvent>,
) -> HashMap<String, Vec<domain::TimelineEvent>> {
    let mut grouped = HashMap::new();
    for event in events {
        grouped
            .entry(event.source_object_id.clone())
            .or_insert_with(Vec::new)
            .push(event);
    }
    grouped
}

fn complete_scan_outputs_match(
    scan: &CompleteAnalysisCandidateScan,
    capability: AnalysisCapability,
    artifacts_by_source: &HashMap<String, Vec<domain::Artifact>>,
    events_by_source: &HashMap<String, Vec<domain::TimelineEvent>>,
) -> bool {
    let artifacts = artifacts_by_source
        .get(&scan.source_object_id)
        .into_iter()
        .flatten()
        .filter(|artifact| artifact_matches(artifact, capability))
        .collect::<Vec<_>>();
    let events = events_by_source
        .get(&scan.source_object_id)
        .into_iter()
        .flatten()
        .filter(|event| event_matches(event, capability))
        .collect::<Vec<_>>();
    if artifacts.len() as u64 != scan.artifact_count
        || events.len() as u64 != scan.timeline_event_count
        || artifacts.iter().any(|artifact| {
            artifact.extractor_version.as_deref() != Some(scan.extractor_version.as_str())
        })
        || events
            .iter()
            .any(|event| event.parser_version.as_deref() != Some(scan.extractor_version.as_str()))
    {
        return false;
    }
    output_digest_for_outputs(artifacts, events) == scan.output_digest
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
