use super::artifact_query::{
    already_has_v1_artifacts, artifacts_by_data_source, count_analysis_artifacts,
};
use super::browser::extract_browser_candidate;
use super::email::extract_email_candidate;
use super::evtx::extract_evtx_candidate;
use super::linux::{extract_linux_candidate, linux_candidate_read_limit};
use super::linux_sections::{linux_artifact_section, LinuxArtifactSection};
use super::registry::extract_registry_candidate;
use super::registry_preload::{preload_registry_context, RegistryPreloadContext};
use super::ExtractionOutcome;
use crate::analysis_service::candidates::{
    evidence_candidates_for_categories, normalize_evidence_path, EvidenceCandidate,
};
use crate::analysis_service::capability::{
    AnalysisCapability, CandidateReadPolicy, LINUX_UMBRELLA_KEY,
};
use crate::analysis_service::error::AnalysisServiceError;
use crate::analysis_service::platforms::analyzer_for;
use chrono::Utc;
use domain::{DataSourcePlatform, FileEntryId};
use persistence_sqlite::repositories::{artifact_repo::ArtifactRepo, timeline_repo::TimelineRepo};
use rusqlite::Connection;
use std::collections::BTreeMap;
use std::io::Read;
use transport::dto::{
    AnalysisExtractionRunDto, AnalysisExtractionSectionRunDto, AnalysisParseStatusDto,
};

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

    fn into_dto(self) -> AnalysisExtractionSectionRunDto {
        let status = extraction_status(self.scanned_count, &self.warnings);
        AnalysisExtractionSectionRunDto {
            key: self.key,
            label: self.label,
            status,
            scanned_count: self.scanned_count,
            artifact_count: self.artifact_count,
            timeline_event_count: self.timeline_event_count,
            warnings: self.warnings,
        }
    }
}

struct ExtractionState {
    artifacts: Vec<domain::Artifact>,
    events: Vec<domain::TimelineEvent>,
    warnings: Vec<String>,
    scanned_count: u64,
    timeline_event_count: u64,
    sections: BTreeMap<String, SectionProgress>,
}

impl ExtractionState {
    fn new(selected: &[AnalysisCapability]) -> Self {
        Self {
            artifacts: Vec::new(),
            events: Vec::new(),
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
        }
    }

    fn record_outcome(&mut self, capability: AnalysisCapability, outcome: ExtractionOutcome) {
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

    fn record_warning(&mut self, capability: AnalysisCapability, warning: String) {
        self.sections
            .entry(capability.key.to_string())
            .or_insert_with(|| SectionProgress::new(capability))
            .warnings
            .push(warning.clone());
        self.warnings.push(warning);
    }
}

pub fn run_analysis_extraction<E: std::fmt::Display>(
    conn: &Connection,
    case_id: &str,
    platform: DataSourcePlatform,
    categories: &[&str],
    mut file_reader: impl FnMut(&FileEntryId) -> Result<Box<dyn Read>, E>,
) -> Result<AnalysisExtractionRunDto, AnalysisServiceError> {
    let selected = analyzer_for(platform)?.select_capabilities(categories)?;
    let discovery_categories = discovery_categories(&selected);
    let candidates = evidence_candidates_for_categories(conn, &discovery_categories)?;
    let preload = preload_registry_context(conn, &candidates, &mut file_reader, |candidate| {
        already_has_v1_artifacts(conn, candidate)
    })?;
    let mut state = ExtractionState::new(&selected);
    if let Some(registry) = selected
        .iter()
        .find(|capability| capability.read_policy == CandidateReadPolicy::RegistryPreload)
    {
        for warning in preload.warnings.iter().cloned() {
            state.record_warning(*registry, warning);
        }
    }
    process_candidates(
        conn,
        candidates,
        &selected,
        &preload,
        &mut file_reader,
        &mut state,
    )?;
    persist_outputs(conn, case_id, &mut state)?;
    build_run_dto(conn, state)
}

fn process_candidates<E: std::fmt::Display>(
    conn: &Connection,
    candidates: Vec<EvidenceCandidate>,
    selected: &[AnalysisCapability],
    preload: &RegistryPreloadContext,
    file_reader: &mut impl FnMut(&FileEntryId) -> Result<Box<dyn Read>, E>,
    state: &mut ExtractionState,
) -> Result<(), AnalysisServiceError> {
    for candidate in candidates {
        let Some(capability) = capability_for_candidate(selected, &candidate) else {
            continue;
        };
        if already_has_v1_artifacts(conn, &candidate)? {
            continue;
        }
        match extract_candidate(&candidate, capability, preload, file_reader) {
            Ok(outcome) => state.record_outcome(capability, outcome),
            Err(warning) => state.record_warning(capability, warning),
        }
    }
    Ok(())
}

fn extract_candidate<E: std::fmt::Display>(
    candidate: &EvidenceCandidate,
    capability: AnalysisCapability,
    preload: &RegistryPreloadContext,
    file_reader: &mut impl FnMut(&FileEntryId) -> Result<Box<dyn Read>, E>,
) -> Result<ExtractionOutcome, String> {
    if capability.read_policy == CandidateReadPolicy::RegistryPreload {
        let bytes = preload
            .registry_bytes(candidate)
            .ok_or_else(|| format!("{} registry bytes not preloaded", candidate.path))?;
        let boot_key = preload.boot_key(candidate);
        let (txlog1, txlog2) = preload.txlogs(candidate);
        return Ok(extract_registry_candidate(
            candidate, bytes, boot_key, txlog1, txlog2,
        ));
    }

    let normalized = normalize_evidence_path(&candidate.path);
    let read_limit = match capability.read_policy {
        CandidateReadPolicy::Bounded(limit) => limit,
        CandidateReadPolicy::LinuxPathAware => linux_candidate_read_limit(&normalized),
        CandidateReadPolicy::RegistryPreload => {
            return Err(format!(
                "{} has an invalid registry read policy",
                candidate.path
            ));
        }
    };
    let bytes = read_candidate_bytes(candidate, read_limit, file_reader)?;
    Ok(match candidate.category.as_str() {
        "BrowserHistory" => extract_browser_candidate(candidate, &bytes),
        "Email" => extract_email_candidate(candidate, &bytes),
        "EventLogs" => extract_evtx_candidate(candidate, &bytes),
        LINUX_UMBRELLA_KEY => extract_linux_candidate(candidate, &bytes),
        _ => ExtractionOutcome::default(),
    })
}

fn read_candidate_bytes<E: std::fmt::Display>(
    candidate: &EvidenceCandidate,
    read_limit: usize,
    file_reader: &mut impl FnMut(&FileEntryId) -> Result<Box<dyn Read>, E>,
) -> Result<Vec<u8>, String> {
    let mut reader = file_reader(&candidate.file_id)
        .map_err(|error| format!("{} read failed: {error}", candidate.path))?;
    let mut bytes = Vec::new();
    reader
        .by_ref()
        .take(read_limit as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("{} read failed: {error}", candidate.path))?;
    Ok(bytes)
}

fn capability_for_candidate(
    selected: &[AnalysisCapability],
    candidate: &EvidenceCandidate,
) -> Option<AnalysisCapability> {
    if candidate.category == LINUX_UMBRELLA_KEY {
        let normalized = normalize_evidence_path(&candidate.path);
        let section = linux_artifact_section(&normalized);
        debug_assert!(LinuxArtifactSection::ALL.contains(&section));
        debug_assert_eq!(LinuxArtifactSection::from_key(section.key()), Some(section));
        let capability = selected
            .iter()
            .find(|item| item.key == section.key())
            .copied();
        return capability;
    }
    selected
        .iter()
        .find(|item| item.candidate_category == candidate.category)
        .copied()
}

fn discovery_categories(selected: &[AnalysisCapability]) -> Vec<&str> {
    let mut categories = Vec::new();
    for capability in selected {
        if !categories.contains(&capability.candidate_category) {
            categories.push(capability.candidate_category);
        }
    }
    categories
}

fn persist_outputs(
    conn: &Connection,
    case_id: &str,
    state: &mut ExtractionState,
) -> Result<(), AnalysisServiceError> {
    if !state.artifacts.is_empty() {
        let by_source = artifacts_by_data_source(std::mem::take(&mut state.artifacts));
        let repo = ArtifactRepo::new(conn);
        for (data_source_id, group) in by_source {
            repo.insert_batch(&group, case_id, &data_source_id)?;
        }
    }
    if !state.events.is_empty() {
        TimelineRepo::new(conn).insert_batch_with_case(&state.events, case_id)?;
    }
    Ok(())
}

fn build_run_dto(
    conn: &Connection,
    state: ExtractionState,
) -> Result<AnalysisExtractionRunDto, AnalysisServiceError> {
    let status = extraction_status(state.scanned_count, &state.warnings);
    Ok(AnalysisExtractionRunDto {
        status,
        scanned_count: state.scanned_count,
        artifact_count: count_analysis_artifacts(conn)?,
        timeline_event_count: state.timeline_event_count,
        sections: state
            .sections
            .into_values()
            .map(SectionProgress::into_dto)
            .collect(),
        generated_at: Utc::now().to_rfc3339(),
        warnings: state.warnings,
    })
}

fn extraction_status(scanned_count: u64, warnings: &[String]) -> AnalysisParseStatusDto {
    match (scanned_count, warnings.is_empty()) {
        (0, true) => AnalysisParseStatusDto::NotFound,
        (0, false) => AnalysisParseStatusDto::Failed,
        (_, true) => AnalysisParseStatusDto::Parsed,
        (_, false) => AnalysisParseStatusDto::Partial,
    }
}
