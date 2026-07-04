use super::artifact_query::{
    already_has_v1_artifacts, artifacts_by_data_source, count_analysis_artifacts,
};
use super::browser::extract_browser_candidate;
use super::email::extract_email_candidate;
use super::evtx::extract_evtx_candidate;
use super::linux::{extract_linux_candidate, linux_candidate_read_limit};
use super::macos::extract_macos_candidate;
use super::registry::extract_registry_candidate;
use super::registry_preload::preload_registry_context;
use super::ExtractionOutcome;
use crate::analysis_service::candidates::{
    evidence_candidates_for_categories, normalize_evidence_path,
};
use crate::analysis_service::error::AnalysisServiceError;
use crate::analysis_service::MAX_ANALYSIS_SOURCE_BYTES;
use chrono::Utc;
use domain::FileEntryId;
use persistence_sqlite::repositories::{artifact_repo::ArtifactRepo, timeline_repo::TimelineRepo};
use rusqlite::Connection;
use std::io::Read;
use transport::dto::{AnalysisExtractionRunDto, AnalysisParseStatusDto};

pub fn run_analysis_extraction<E: std::fmt::Display>(
    conn: &Connection,
    case_id: &str,
    categories: &[&str],
    mut file_reader: impl FnMut(&FileEntryId) -> Result<Box<dyn Read>, E>,
) -> Result<AnalysisExtractionRunDto, AnalysisServiceError> {
    let generated_at = Utc::now().to_rfc3339();
    let selected = if categories.is_empty() {
        vec![
            "Registry",
            "BrowserHistory",
            "Email",
            "EventLogs",
            "LinuxArtifacts",
            "MacArtifacts",
        ]
    } else {
        categories.to_vec()
    };
    let candidates = evidence_candidates_for_categories(conn, &selected)?;
    let mut artifacts = Vec::new();
    let mut events = Vec::new();
    let mut warnings = Vec::new();
    let mut scanned_count = 0u64;

    let preload = preload_registry_context(conn, &candidates, &mut file_reader, |candidate| {
        already_has_v1_artifacts(conn, candidate)
    })?;
    warnings.extend(preload.warnings.iter().cloned());

    for candidate in candidates {
        if !is_supported_analysis_category(&candidate.category) {
            continue;
        }
        if already_has_v1_artifacts(conn, &candidate)? {
            continue;
        }

        let outcome = match candidate.category.as_str() {
            "Registry" => {
                let Some(bytes) = preload.registry_bytes(&candidate) else {
                    warnings.push(format!("{} registry bytes not preloaded", candidate.path));
                    continue;
                };
                let boot_key = preload.boot_key(&candidate);
                let (txlog1, txlog2) = preload.txlogs(&candidate);
                scanned_count += 1;
                extract_registry_candidate(&candidate, bytes, boot_key, txlog1, txlog2)
            }
            "BrowserHistory" | "Email" | "EventLogs" | "LinuxArtifacts" | "MacArtifacts" => {
                let mut reader = match file_reader(&candidate.file_id) {
                    Ok(reader) => reader,
                    Err(err) => {
                        warnings.push(format!("{} read failed: {}", candidate.path, err));
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
                    warnings.push(format!("{} read failed: {}", candidate.path, err));
                    continue;
                }
                scanned_count += 1;
                match candidate.category.as_str() {
                    "BrowserHistory" => extract_browser_candidate(&candidate, &bytes),
                    "Email" => extract_email_candidate(&candidate, &bytes),
                    "EventLogs" => extract_evtx_candidate(&candidate, &bytes),
                    "LinuxArtifacts" => extract_linux_candidate(&candidate, &bytes),
                    "MacArtifacts" => extract_macos_candidate(&candidate, &bytes),
                    _ => ExtractionOutcome::default(),
                }
            }
            _ => ExtractionOutcome::default(),
        };
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
        generated_at,
        warnings,
    })
}

fn is_supported_analysis_category(category: &str) -> bool {
    matches!(
        category,
        "Registry" | "BrowserHistory" | "Email" | "EventLogs" | "LinuxArtifacts" | "MacArtifacts"
    )
}

fn analysis_candidate_read_limit(category: &str, normalized_path: &str) -> usize {
    if category == "LinuxArtifacts" {
        linux_candidate_read_limit(normalized_path)
    } else {
        MAX_ANALYSIS_SOURCE_BYTES
    }
}
