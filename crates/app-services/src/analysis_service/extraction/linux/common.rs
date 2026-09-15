use crate::analysis_service::candidates::EvidenceCandidate;
use crate::analysis_service::extraction::linux_sections::{
    linux_artifact_route, LinuxCandidateSupport,
};
use crate::analysis_service::MAX_ANALYSIS_SOURCE_BYTES;
use flate2::read::GzDecoder;
use serde_json::Value;
use std::collections::BTreeMap;
use std::io::Read;

pub(super) const MAX_TEXT_LOG_EVENTS_PER_SOURCE: usize = 10_000;
pub(super) const MAX_WEB_ERROR_LOG_EVENTS_PER_SOURCE: usize = 2_000;
pub(super) const MAX_MYSQL_LOG_EVENTS_PER_SOURCE: usize = 2_000;
pub(super) const MAX_JOURNAL_EVENTS_PER_SOURCE: usize = 50_000;
pub(super) const MAX_LOGIN_EVENTS_PER_SOURCE: usize = 20_000;
pub(super) const MAX_SHELL_HISTORY_EVENTS_PER_SOURCE: usize = 20_000;
pub(super) const MAX_PACKAGE_EVENTS_PER_SOURCE: usize = 20_000;
pub(in crate::analysis_service::extraction) fn linux_candidate_read_limit(
    normalized_path: &str,
) -> usize {
    linux_artifact_route(normalized_path).read_limit
}

pub(in crate::analysis_service::extraction) fn linux_candidate_support(
    normalized_path: &str,
) -> LinuxCandidateSupport {
    linux_artifact_route(normalized_path).support
}

/// Why decoded gzip output does not cover the full source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum GzipTruncation {
    /// Decoded output exceeded the analysis byte cap and was cut.
    OutputCap,
    /// The compressed stream ended prematurely (typical for rotated logs
    /// whose compressed bytes hit the read limit mid-stream); the decoded
    /// prefix is still usable.
    TruncatedStream,
}

pub(super) fn decode_gzip(
    bytes: &[u8],
) -> Result<(Vec<u8>, Option<GzipTruncation>), std::io::Error> {
    let mut decoder = GzDecoder::new(bytes);
    let mut decoded = Vec::new();
    let read_result = decoder
        .by_ref()
        .take(MAX_ANALYSIS_SOURCE_BYTES as u64 + 1)
        .read_to_end(&mut decoded);
    if let Err(error) = read_result {
        // A truncated compressed stream still yields a valid decoded prefix;
        // genuinely corrupt input (bad header, bad deflate blocks) stays an
        // error, as does a premature end with no decodable payload at all.
        if error.kind() != std::io::ErrorKind::UnexpectedEof || decoded.is_empty() {
            return Err(error);
        }
        return Ok((decoded, Some(GzipTruncation::TruncatedStream)));
    }
    if decoded.len() > MAX_ANALYSIS_SOURCE_BYTES {
        decoded.truncate(MAX_ANALYSIS_SOURCE_BYTES);
        return Ok((decoded, Some(GzipTruncation::OutputCap)));
    }
    Ok((decoded, None))
}

/// Cap the number of events materialized from a single source, recording a
/// warning with the skipped count when the cap is hit.
pub(super) fn cap_source_events<T>(
    candidate: &EvidenceCandidate,
    label: &str,
    limit: usize,
    events: Vec<T>,
    warnings: &mut Vec<String>,
) -> Vec<T> {
    if events.len() <= limit {
        return events;
    }
    let skipped = events.len() - limit;
    warnings.push(format!(
        "{} {} emitted first {} records only ({} more skipped)",
        candidate.path, label, limit, skipped
    ));
    events.into_iter().take(limit).collect()
}

/// Surface an explicit warning when a log source shows signs of a format the
/// parser does not understand: no records at all from non-blank content, or
/// more than a quarter of the lines (and at least five) left unparsed. Both
/// cases previously failed silently — zero records, zero warnings.
pub(super) fn warn_on_parse_gap(
    candidate: &EvidenceCandidate,
    label: &str,
    total_lines: u64,
    parsed_lines: u64,
    warnings: &mut Vec<String>,
) {
    let unparsed = total_lines.saturating_sub(parsed_lines);
    if total_lines > 0 && parsed_lines == 0 {
        warnings.push(format!(
            "{} {} produced no records from {} non-blank lines; the file likely uses a custom or unsupported log format",
            candidate.path, label, total_lines
        ));
    } else if unparsed >= 5 && unparsed > total_lines / 4 {
        warnings.push(format!(
            "{} {} parsed {} of {} non-blank lines; the remaining {} lines did not match the expected format (custom log_format?)",
            candidate.path, label, parsed_lines, total_lines, unparsed
        ));
    }
}

pub(super) fn insert_opt(attrs: &mut BTreeMap<String, Value>, key: &str, value: Option<String>) {
    if let Some(value) = value {
        if !value.is_empty() {
            attrs.insert(key.to_string(), Value::String(value));
        }
    }
}

pub(super) fn insert_string_array(
    attrs: &mut BTreeMap<String, Value>,
    key: &str,
    values: &[String],
) {
    if !values.is_empty() {
        attrs.insert(
            key.to_string(),
            Value::Array(values.iter().cloned().map(Value::String).collect()),
        );
    }
}

pub(super) fn truncate(value: &str, max_len: usize) -> String {
    if value.chars().count() <= max_len {
        value.to_string()
    } else {
        format!("{}…", value.chars().take(max_len).collect::<String>())
    }
}

#[cfg(test)]
#[path = "../../../../tests/unit/analysis_service/extraction/linux/common.rs"]
mod tests;
