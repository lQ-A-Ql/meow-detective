use super::artifact_query::{
    already_has_v1_artifacts, artifacts_by_data_source, count_analysis_artifacts,
};
use super::browser::extract_browser_candidate;
use super::email::extract_email_candidate;
use super::evtx::extract_evtx_candidate;
use super::linux::{extract_linux_candidate, linux_candidate_read_limit};
use super::linux_sections::{linux_artifact_section, LinuxArtifactSection};
use super::registry::extract_registry_candidate;
use super::registry_preload::preload_registry_context;
use super::ExtractionOutcome;
use crate::analysis_service::candidates::{
    ensure_supported_analysis_categories, evidence_candidates_for_categories,
    normalize_evidence_path,
};
use crate::analysis_service::error::AnalysisServiceError;
use crate::analysis_service::MAX_ANALYSIS_SOURCE_BYTES;
use chrono::Utc;
use domain::FileEntryId;
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
    fn new(key: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            label: label.into(),
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

    fn record_warning(&mut self, warning: String) {
        self.warnings.push(warning);
    }

    fn into_dto(self) -> AnalysisExtractionSectionRunDto {
        let status = if self.scanned_count == 0 && self.warnings.is_empty() {
            AnalysisParseStatusDto::NotFound
        } else if self.scanned_count == 0 {
            AnalysisParseStatusDto::Failed
        } else if self.warnings.is_empty() {
            AnalysisParseStatusDto::Parsed
        } else {
            AnalysisParseStatusDto::Partial
        };
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

pub fn run_analysis_extraction<E: std::fmt::Display>(
    conn: &Connection,
    case_id: &str,
    categories: &[&str],
    mut file_reader: impl FnMut(&FileEntryId) -> Result<Box<dyn Read>, E>,
) -> Result<AnalysisExtractionRunDto, AnalysisServiceError> {
    ensure_supported_analysis_categories(categories)?;
    let generated_at = Utc::now().to_rfc3339();
    let selected = if categories.is_empty() {
        vec![
            "Registry",
            "BrowserHistory",
            "Email",
            "EventLogs",
            "LinuxArtifacts",
        ]
    } else {
        categories.to_vec()
    };
    let discovery_categories = discovery_categories_for_selection(&selected);
    let candidates = evidence_candidates_for_categories(conn, &discovery_categories)?;
    let mut artifacts = Vec::new();
    let mut events = Vec::new();
    let mut warnings = Vec::new();
    let mut scanned_count = 0u64;
    let mut section_progress = initial_section_progress(&selected);

    let preload = preload_registry_context(conn, &candidates, &mut file_reader, |candidate| {
        already_has_v1_artifacts(conn, candidate)
    })?;
    warnings.extend(preload.warnings.iter().cloned());

    for candidate in candidates {
        if !is_supported_analysis_category(&candidate.category) {
            continue;
        }
        if !candidate_matches_selection(&candidate, &selected) {
            continue;
        }
        if already_has_v1_artifacts(conn, &candidate)? {
            continue;
        }

        let section = section_for_candidate(&candidate);
        let outcome = match candidate.category.as_str() {
            "Registry" => {
                let Some(bytes) = preload.registry_bytes(&candidate) else {
                    let warning = format!("{} registry bytes not preloaded", candidate.path);
                    section_progress
                        .entry(section.key.to_string())
                        .or_insert_with(|| SectionProgress::new(section.key, section.label))
                        .record_warning(warning.clone());
                    warnings.push(warning);
                    continue;
                };
                let boot_key = preload.boot_key(&candidate);
                let (txlog1, txlog2) = preload.txlogs(&candidate);
                scanned_count += 1;
                extract_registry_candidate(&candidate, bytes, boot_key, txlog1, txlog2)
            }
            "BrowserHistory" | "Email" | "EventLogs" | "LinuxArtifacts" => {
                let mut reader = match file_reader(&candidate.file_id) {
                    Ok(reader) => reader,
                    Err(err) => {
                        let warning = format!("{} read failed: {}", candidate.path, err);
                        section_progress
                            .entry(section.key.to_string())
                            .or_insert_with(|| SectionProgress::new(section.key, section.label))
                            .record_warning(warning.clone());
                        warnings.push(warning);
                        continue;
                    }
                };
                let normalized = normalize_evidence_path(&candidate.path);
                let read_limit = analysis_candidate_read_limit(&candidate.category, &normalized);
                let mut bytes = Vec::new();
                if let Err(err) = reader
                    .by_ref()
                    .take(read_limit as u64)
                    .read_to_end(&mut bytes)
                {
                    let warning = format!("{} read failed: {}", candidate.path, err);
                    section_progress
                        .entry(section.key.to_string())
                        .or_insert_with(|| SectionProgress::new(section.key, section.label))
                        .record_warning(warning.clone());
                    warnings.push(warning);
                    continue;
                }
                scanned_count += 1;
                match candidate.category.as_str() {
                    "BrowserHistory" => extract_browser_candidate(&candidate, &bytes),
                    "Email" => extract_email_candidate(&candidate, &bytes),
                    "EventLogs" => extract_evtx_candidate(&candidate, &bytes),
                    "LinuxArtifacts" => extract_linux_candidate(&candidate, &bytes),
                    _ => ExtractionOutcome::default(),
                }
            }
            _ => ExtractionOutcome::default(),
        };
        section_progress
            .entry(section.key.to_string())
            .or_insert_with(|| SectionProgress::new(section.key, section.label))
            .record_scan(&outcome);
        warnings.extend(outcome.warnings);
        artifacts.extend(outcome.artifacts);
        events.extend(outcome.timeline_events);
    }

    if !artifacts.is_empty() {
        let by_source = artifacts_by_data_source(artifacts);
        let repo = ArtifactRepo::new(conn);
        for (data_source_id, group) in by_source {
            repo.insert_batch(&group, case_id, &data_source_id)?;
        }
    }
    if !events.is_empty() {
        TimelineRepo::new(conn).insert_batch_with_case(&events, case_id)?;
    }

    let artifact_count = count_analysis_artifacts(conn)?;
    Ok(AnalysisExtractionRunDto {
        status: if scanned_count == 0 {
            AnalysisParseStatusDto::NotFound
        } else if warnings.is_empty() {
            AnalysisParseStatusDto::Parsed
        } else {
            AnalysisParseStatusDto::Partial
        },
        scanned_count,
        artifact_count,
        timeline_event_count: events.len() as u64,
        sections: section_progress
            .into_values()
            .map(SectionProgress::into_dto)
            .collect(),
        generated_at,
        warnings,
    })
}

fn is_supported_analysis_category(category: &str) -> bool {
    matches!(
        category,
        "Registry" | "BrowserHistory" | "Email" | "EventLogs" | "LinuxArtifacts"
    )
}

fn discovery_categories_for_selection<'a>(selected: &[&'a str]) -> Vec<&'a str> {
    let mut categories = Vec::new();
    for category in selected {
        let discovery_category = if LinuxArtifactSection::from_key(category).is_some() {
            "LinuxArtifacts"
        } else {
            category
        };
        if !categories.contains(&discovery_category) {
            categories.push(discovery_category);
        }
    }
    categories
}

fn candidate_matches_selection(
    candidate: &crate::analysis_service::candidates::EvidenceCandidate,
    selected: &[&str],
) -> bool {
    if candidate.category != "LinuxArtifacts" {
        return selected
            .iter()
            .any(|category| *category == candidate.category);
    }
    selected.contains(&"LinuxArtifacts")
        || selected.iter().any(|category| {
            LinuxArtifactSection::from_key(category).is_some_and(|section| {
                let normalized = normalize_evidence_path(&candidate.path);
                linux_artifact_section(&normalized) == section
            })
        })
}

fn initial_section_progress(selected: &[&str]) -> BTreeMap<String, SectionProgress> {
    let mut sections = BTreeMap::new();
    for category in selected {
        if *category == "LinuxArtifacts" {
            for section in LinuxArtifactSection::ALL {
                sections.insert(
                    section.key().to_string(),
                    SectionProgress::new(section.key(), section.label()),
                );
            }
        } else if let Some(section) = LinuxArtifactSection::from_key(category) {
            sections.insert(
                section.key().to_string(),
                SectionProgress::new(section.key(), section.label()),
            );
        } else if is_supported_analysis_category(category) {
            let section = generic_section_for_category(category);
            sections.insert(
                section.key.to_string(),
                SectionProgress::new(section.key, section.label),
            );
        }
    }
    sections
}

#[derive(Debug, Clone, Copy)]
struct ExtractionSection {
    key: &'static str,
    label: &'static str,
}

fn section_for_candidate(
    candidate: &crate::analysis_service::candidates::EvidenceCandidate,
) -> ExtractionSection {
    if candidate.category == "LinuxArtifacts" {
        let normalized = normalize_evidence_path(&candidate.path);
        let section = linux_artifact_section(&normalized);
        ExtractionSection {
            key: section.key(),
            label: section.label(),
        }
    } else {
        generic_section_for_category(&candidate.category)
    }
}

fn generic_section_for_category(category: &str) -> ExtractionSection {
    match category {
        "Registry" => ExtractionSection {
            key: "Registry",
            label: "Windows Registry",
        },
        "BrowserHistory" => ExtractionSection {
            key: "BrowserHistory",
            label: "Browser History",
        },
        "Email" => ExtractionSection {
            key: "Email",
            label: "Email",
        },
        "EventLogs" => ExtractionSection {
            key: "EventLogs",
            label: "Windows Event Logs",
        },
        _ => ExtractionSection {
            key: "Unknown",
            label: "Unknown",
        },
    }
}

fn analysis_candidate_read_limit(category: &str, normalized_path: &str) -> usize {
    if category == "LinuxArtifacts" {
        linux_candidate_read_limit(normalized_path)
    } else {
        MAX_ANALYSIS_SOURCE_BYTES
    }
}
