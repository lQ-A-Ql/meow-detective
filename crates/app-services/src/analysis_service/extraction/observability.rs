use crate::analysis_service::candidates::{
    evidence_candidates_for_categories, normalize_evidence_path,
};
use crate::analysis_service::error::AnalysisServiceError;
use crate::analysis_service::extraction::linux::{
    linux_candidate_read_limit, linux_candidate_support,
};
use crate::analysis_service::extraction::linux_sections::LinuxCandidateSupport;
use rusqlite::Connection;
use std::collections::HashMap;
use transport::dto::AnalysisParseStatusDto;

pub(super) struct LinuxSummaryObservability {
    pub(super) candidate_count: u64,
    pub(super) coverage_ratio: f32,
    pub(super) truncated: bool,
    pub(super) warnings: Vec<String>,
}

pub(super) fn linux_summary_observability(
    conn: &Connection,
    total_count: u64,
) -> Result<LinuxSummaryObservability, AnalysisServiceError> {
    let candidates = evidence_candidates_for_categories(conn, &["LinuxArtifacts"])?;
    let candidate_count = candidates.len() as u64;
    let parsed_sources = linux_parsed_source_ids(conn)?;
    let parsed_candidate_count = candidates
        .iter()
        .filter(|candidate| parsed_sources.contains_key(&candidate.file_id.0))
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
    let mut warnings = Vec::new();
    if candidate_count > 0 && total_count == 0 {
        warnings.push(format!(
            "Found {candidate_count} Linux artifact candidate(s), but no structured artifacts have been extracted yet. Run LinuxArtifacts extraction or review parser support warnings."
        ));
    } else if candidate_count > parsed_candidate_count && total_count > 0 {
        warnings.push(format!(
            "Structured output coverage is {parsed_candidate_count} of {candidate_count} Linux artifact candidate source(s). Sources without structured output may be empty, unsupported, or handled by the generic text parser; this is a coverage summary, not live extraction progress."
        ));
    }

    let mut truncated = false;
    let unsupported_count = candidates
        .iter()
        .filter(|candidate| {
            matches!(
                linux_candidate_support(&normalize_evidence_path(&candidate.path)),
                LinuxCandidateSupport::Unsupported
            )
        })
        .count() as u64;
    let text_fallback_count = candidates
        .iter()
        .filter(|candidate| {
            matches!(
                linux_candidate_support(&normalize_evidence_path(&candidate.path)),
                LinuxCandidateSupport::TextFallback
            )
        })
        .count() as u64;

    for candidate in &candidates {
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
        warnings.push(format!(
            "{unsupported_count} Linux candidate source(s) are detected for coverage but do not yet have a structured first-pass parser."
        ));
    }
    if text_fallback_count > 0 {
        warnings.push(format!(
            "{text_fallback_count} Linux text log/source file(s) are covered with generic line-level extraction."
        ));
    }

    Ok(LinuxSummaryObservability {
        candidate_count,
        coverage_ratio,
        truncated,
        warnings,
    })
}

fn linux_parsed_source_ids(
    conn: &Connection,
) -> Result<HashMap<String, u64>, AnalysisServiceError> {
    let mut stmt = conn.prepare(
        "SELECT source_object_id, COUNT(*)
         FROM artifacts
         WHERE artifact_type IN ('LinuxJournal', 'LinuxWtmp', 'LinuxBashCommand', 'LinuxAptEvent', 'LinuxCronJob', 'LinuxSudoEvent', 'LinuxSystemConfig', 'LinuxWebSite', 'LinuxWebAccessLog', 'LinuxWebErrorLog', 'LinuxWebFinding', 'LinuxMysqlConfig', 'LinuxMysqlLogEntry', 'LinuxMysqlFinding')
           AND source_object_id IS NOT NULL
         GROUP BY source_object_id",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as u64))
    })?;
    let mut ids = HashMap::new();
    for row in rows {
        let (source_id, count) = row?;
        ids.insert(source_id, count);
    }
    Ok(ids)
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
