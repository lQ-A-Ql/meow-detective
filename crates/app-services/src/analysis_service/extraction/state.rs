use super::artifact_query::count_analysis_artifacts;
use super::ExtractionOutcome;
use crate::analysis_service::candidates::EvidenceCandidate;
use crate::analysis_service::capability::AnalysisCapability;
use crate::analysis_service::error::AnalysisServiceError;
use crate::analysis_service::ANALYSIS_EXTRACTOR_VERSION;
use persistence_sqlite::repositories::analysis_scan_repo::{
    AnalysisScanRepo, CleanAnalysisCandidateScan, CompleteAnalysisCandidateScan,
    DiagnosticAnalysisCandidateScan,
};
use persistence_sqlite::repositories::{artifact_repo::ArtifactRepo, timeline_repo::TimelineRepo};
use rusqlite::Connection;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap, HashSet};
use transport::dto::{
    AnalysisExtractionRunDto, AnalysisExtractionSectionRunDto, AnalysisParseStatusDto,
};

pub(super) type AnalysisCheckpointKey = (String, String, u64, String);
pub(super) type CleanScanKeys = HashSet<AnalysisCheckpointKey>;
pub(super) type DiagnosticScanKeys = HashMap<AnalysisCheckpointKey, Vec<String>>;

#[derive(Debug, Clone)]
struct SectionProgress {
    key: String,
    label: String,
    scanned_count: u64,
    artifact_count: u64,
    timeline_event_count: u64,
    warnings: Vec<String>,
}

impl SectionProgress {
    fn new(capability: AnalysisCapability) -> Self {
        Self {
            key: capability.key.to_string(),
            label: capability.section_label.to_string(),
            scanned_count: 0,
            artifact_count: 0,
            timeline_event_count: 0,
            warnings: Vec::new(),
        }
    }

    fn record_scan(&mut self, outcome: &ExtractionOutcome) {
        self.scanned_count += 1;
        self.artifact_count += outcome.artifacts.len() as u64;
        self.timeline_event_count += outcome.timeline_events.len() as u64;
        self.warnings.extend(outcome.warnings.iter().cloned());
    }

    fn record_checkpoint(
        &mut self,
        artifact_count: u64,
        timeline_event_count: u64,
        warnings: &[String],
    ) {
        self.scanned_count += 1;
        self.artifact_count += artifact_count;
        self.timeline_event_count += timeline_event_count;
        self.warnings.extend(warnings.iter().cloned());
    }

    fn into_dto(self) -> AnalysisExtractionSectionRunDto {
        AnalysisExtractionSectionRunDto {
            key: self.key,
            label: self.label,
            status: extraction_status(self.scanned_count, &self.warnings),
            scanned_count: self.scanned_count,
            artifact_count: self.artifact_count,
            timeline_event_count: self.timeline_event_count,
            warnings: self.warnings,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct AnalysisOutputReplacement {
    pub(super) source_object_id: String,
    pub(super) producer_prefix: &'static str,
}

pub(super) struct ExtractionState {
    pub(super) artifacts: Vec<domain::Artifact>,
    pub(super) events: Vec<domain::TimelineEvent>,
    pub(super) replacements: Vec<AnalysisOutputReplacement>,
    pub(super) warnings: Vec<String>,
    pub(super) scanned_count: u64,
    pub(super) timeline_event_count: u64,
    sections: BTreeMap<String, SectionProgress>,
    pub(super) clean_scans: Vec<CleanAnalysisCandidateScan>,
    pub(super) diagnostic_scans: Vec<DiagnosticAnalysisCandidateScan>,
    pub(super) complete_scans: Vec<CompleteAnalysisCandidateScan>,
    pub(super) checkpoint_hit_count: u64,
    pub(super) retryable_failure_count: u64,
}

impl ExtractionState {
    pub(super) fn new(selected: &[AnalysisCapability]) -> Self {
        Self {
            artifacts: Vec::new(),
            events: Vec::new(),
            replacements: Vec::new(),
            warnings: Vec::new(),
            scanned_count: 0,
            timeline_event_count: 0,
            sections: selected
                .iter()
                .map(|capability| {
                    (
                        capability.key.to_string(),
                        SectionProgress::new(*capability),
                    )
                })
                .collect(),
            clean_scans: Vec::new(),
            diagnostic_scans: Vec::new(),
            complete_scans: Vec::new(),
            checkpoint_hit_count: 0,
            retryable_failure_count: 0,
        }
    }

    pub(super) fn record_outcome(
        &mut self,
        capability: AnalysisCapability,
        candidate: &EvidenceCandidate,
        outcome: ExtractionOutcome,
    ) {
        self.replacements.push(AnalysisOutputReplacement {
            source_object_id: candidate.file_id.0.clone(),
            producer_prefix: capability.producer_prefix(),
        });
        if outcome.warnings.is_empty()
            && outcome.artifacts.is_empty()
            && outcome.timeline_events.is_empty()
        {
            self.clean_scans.push(CleanAnalysisCandidateScan {
                source_object_id: candidate.file_id.0.clone(),
                capability_key: capability.key.to_string(),
                extractor_version: ANALYSIS_EXTRACTOR_VERSION.to_string(),
                source_size: candidate.size,
                content_identity: candidate.content_identity.clone(),
            });
        } else if !outcome.warnings.is_empty()
            && outcome.artifacts.is_empty()
            && outcome.timeline_events.is_empty()
        {
            self.diagnostic_scans.push(DiagnosticAnalysisCandidateScan {
                source_object_id: candidate.file_id.0.clone(),
                capability_key: capability.key.to_string(),
                extractor_version: ANALYSIS_EXTRACTOR_VERSION.to_string(),
                source_size: candidate.size,
                content_identity: candidate.content_identity.clone(),
                warnings: outcome.warnings.clone(),
            });
        } else {
            self.complete_scans.push(CompleteAnalysisCandidateScan {
                source_object_id: candidate.file_id.0.clone(),
                capability_key: capability.key.to_string(),
                extractor_version: ANALYSIS_EXTRACTOR_VERSION.to_string(),
                source_size: candidate.size,
                content_identity: candidate.content_identity.clone(),
                artifact_count: outcome.artifacts.len() as u64,
                timeline_event_count: outcome.timeline_events.len() as u64,
                output_digest: output_digest(&outcome),
                warnings: outcome.warnings.clone(),
            });
        }
        self.record_observation(capability, outcome);
    }

    pub(super) fn replay_diagnostic(
        &mut self,
        capability: AnalysisCapability,
        warnings: &[String],
    ) {
        self.checkpoint_hit_count += 1;
        self.record_observation(
            capability,
            ExtractionOutcome {
                warnings: warnings.to_vec(),
                ..ExtractionOutcome::default()
            },
        );
    }

    pub(super) fn replay_clean(&mut self, capability: AnalysisCapability) {
        self.checkpoint_hit_count += 1;
        self.record_observation(capability, ExtractionOutcome::default());
    }

    pub(super) fn replay_complete(
        &mut self,
        capability: AnalysisCapability,
        scan: &CompleteAnalysisCandidateScan,
    ) {
        self.checkpoint_hit_count += 1;
        self.scanned_count += 1;
        self.timeline_event_count = self
            .timeline_event_count
            .saturating_add(scan.timeline_event_count);
        self.sections
            .entry(capability.key.to_string())
            .or_insert_with(|| SectionProgress::new(capability))
            .record_checkpoint(
                scan.artifact_count,
                scan.timeline_event_count,
                &scan.warnings,
            );
        self.warnings.extend(scan.warnings.iter().cloned());
    }

    pub(super) fn record_warning(&mut self, capability: AnalysisCapability, warning: String) {
        self.retryable_failure_count += 1;
        self.sections
            .entry(capability.key.to_string())
            .or_insert_with(|| SectionProgress::new(capability))
            .warnings
            .push(warning.clone());
        self.warnings.push(warning);
    }

    pub(super) fn has_pending_outputs(&self) -> bool {
        !self.replacements.is_empty()
            || !self.artifacts.is_empty()
            || !self.events.is_empty()
            || !self.clean_scans.is_empty()
            || !self.diagnostic_scans.is_empty()
            || !self.complete_scans.is_empty()
    }

    pub(super) fn into_dto(
        self,
        conn: &Connection,
        generated_at: String,
    ) -> Result<AnalysisExtractionRunDto, AnalysisServiceError> {
        let status = extraction_status(self.scanned_count, &self.warnings);
        Ok(AnalysisExtractionRunDto {
            status,
            scanned_count: self.scanned_count,
            checkpoint_hit_count: self.checkpoint_hit_count,
            artifact_count: count_analysis_artifacts(conn)?,
            timeline_event_count: self.timeline_event_count,
            sections: self
                .sections
                .into_values()
                .map(SectionProgress::into_dto)
                .collect(),
            generated_at,
            warnings: self.warnings,
        })
    }

    fn record_observation(&mut self, capability: AnalysisCapability, outcome: ExtractionOutcome) {
        self.scanned_count += 1;
        self.timeline_event_count += outcome.timeline_events.len() as u64;
        self.sections
            .entry(capability.key.to_string())
            .or_insert_with(|| SectionProgress::new(capability))
            .record_scan(&outcome);
        self.warnings.extend(outcome.warnings.iter().cloned());
        self.artifacts.extend(outcome.artifacts);
        self.events.extend(outcome.timeline_events);
    }
}

pub(super) fn existing_clean_scan_keys(
    conn: &Connection,
) -> Result<CleanScanKeys, AnalysisServiceError> {
    let mut valid = HashSet::new();
    for scan in AnalysisScanRepo::new(conn).list_clean_for_version(ANALYSIS_EXTRACTOR_VERSION)? {
        if scan_has_no_outputs(conn, &scan.source_object_id, &scan.capability_key)? {
            valid.insert((
                scan.source_object_id,
                scan.capability_key,
                scan.source_size,
                scan.content_identity,
            ));
        }
    }
    Ok(valid)
}

pub(super) fn existing_diagnostic_scan_keys(
    conn: &Connection,
) -> Result<DiagnosticScanKeys, AnalysisServiceError> {
    let mut valid = HashMap::new();
    for scan in
        AnalysisScanRepo::new(conn).list_diagnostics_for_version(ANALYSIS_EXTRACTOR_VERSION)?
    {
        if scan_has_no_outputs(conn, &scan.source_object_id, &scan.capability_key)? {
            valid.insert(
                (
                    scan.source_object_id,
                    scan.capability_key,
                    scan.source_size,
                    scan.content_identity,
                ),
                scan.warnings,
            );
        }
    }
    Ok(valid)
}

fn scan_has_no_outputs(
    conn: &Connection,
    source_object_id: &str,
    capability_key: &str,
) -> Result<bool, AnalysisServiceError> {
    let Some(capability) = crate::analysis_service::capability::find_capability(capability_key)
    else {
        return Ok(false);
    };
    let prefix = capability.producer_prefix();
    Ok(
        ArtifactRepo::new(conn).count_analysis_outputs(source_object_id, prefix)? == 0
            && TimelineRepo::new(conn).count_analysis_outputs(source_object_id, prefix)? == 0,
    )
}

fn output_digest(outcome: &ExtractionOutcome) -> String {
    output_digest_for_outputs(&outcome.artifacts, &outcome.timeline_events)
}

pub(super) fn output_digest_for_outputs<'a>(
    artifacts: impl IntoIterator<Item = &'a domain::Artifact>,
    timeline_events: impl IntoIterator<Item = &'a domain::TimelineEvent>,
) -> String {
    let mut artifact_records = artifacts
        .into_iter()
        .map(artifact_digest_record)
        .collect::<Vec<_>>();
    artifact_records.sort_unstable();
    let mut event_records = timeline_events
        .into_iter()
        .map(timeline_digest_record)
        .collect::<Vec<_>>();
    event_records.sort_unstable();

    let mut hasher = Sha256::new();
    update_digest_field(&mut hasher, b"analysis-output-v2");
    hasher.update((artifact_records.len() as u64).to_le_bytes());
    for record in artifact_records {
        update_digest_field(&mut hasher, &record);
    }
    hasher.update((event_records.len() as u64).to_le_bytes());
    for record in event_records {
        update_digest_field(&mut hasher, &record);
    }
    hex::encode(hasher.finalize())
}

fn artifact_digest_record(artifact: &domain::Artifact) -> Vec<u8> {
    let mut record = Vec::new();
    append_record_field(&mut record, artifact.family.as_bytes());
    append_optional_record_field(
        &mut record,
        artifact.source_object_id.as_ref().map(|id| id.0.as_bytes()),
    );
    append_optional_record_field(
        &mut record,
        artifact.extractor_id.as_deref().map(str::as_bytes),
    );
    append_optional_record_field(
        &mut record,
        artifact.extractor_version.as_deref().map(str::as_bytes),
    );
    append_optional_f32(&mut record, artifact.confidence);
    append_optional_record_field(
        &mut record,
        artifact.source_attribution.as_deref().map(str::as_bytes),
    );
    append_record_field(&mut record, artifact.title.as_bytes());
    append_record_field(&mut record, artifact.summary.as_bytes());
    append_record_field(
        &mut record,
        serde_json::to_string(&artifact.attrs)
            .unwrap_or_else(|_| "{}".to_string())
            .as_bytes(),
    );
    record
}

fn timeline_digest_record(event: &domain::TimelineEvent) -> Vec<u8> {
    let mut record = Vec::new();
    append_record_field(&mut record, event.source_object_id.as_bytes());
    append_record_field(&mut record, event.event_type.as_bytes());
    append_record_field(&mut record, event.timestamp.to_rfc3339().as_bytes());
    append_record_field(&mut record, event.title.as_bytes());
    append_record_field(&mut record, event.description.as_bytes());
    append_optional_record_field(&mut record, event.parser_id.as_deref().map(str::as_bytes));
    append_optional_record_field(
        &mut record,
        event.parser_version.as_deref().map(str::as_bytes),
    );
    append_optional_f32(&mut record, event.confidence);
    append_optional_record_field(
        &mut record,
        event.source_attribution.as_deref().map(str::as_bytes),
    );
    append_record_field(
        &mut record,
        serde_json::to_string(&event.attrs)
            .unwrap_or_else(|_| "{}".to_string())
            .as_bytes(),
    );
    record
}

fn append_optional_record_field(record: &mut Vec<u8>, value: Option<&[u8]>) {
    match value {
        Some(value) => {
            record.push(1);
            append_record_field(record, value);
        }
        None => record.push(0),
    }
}

fn append_optional_f32(record: &mut Vec<u8>, value: Option<f32>) {
    match value {
        Some(value) => {
            record.push(1);
            record.extend_from_slice(&value.to_bits().to_le_bytes());
        }
        None => record.push(0),
    }
}

fn append_record_field(record: &mut Vec<u8>, value: &[u8]) {
    record.extend_from_slice(&(value.len() as u64).to_le_bytes());
    record.extend_from_slice(value);
}

fn update_digest_field(hasher: &mut Sha256, value: &[u8]) {
    hasher.update((value.len() as u64).to_le_bytes());
    hasher.update(value);
}

fn extraction_status(scanned_count: u64, warnings: &[String]) -> AnalysisParseStatusDto {
    match (scanned_count, warnings.is_empty()) {
        (0, true) => AnalysisParseStatusDto::NotFound,
        (0, false) => AnalysisParseStatusDto::Failed,
        (_, true) => AnalysisParseStatusDto::Parsed,
        (_, false) => AnalysisParseStatusDto::Partial,
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/analysis_service/extraction/state.rs"]
mod tests;
