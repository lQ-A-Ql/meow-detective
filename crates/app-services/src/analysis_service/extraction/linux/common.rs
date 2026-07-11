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

pub(super) fn decode_gzip(bytes: &[u8]) -> Result<(Vec<u8>, bool), std::io::Error> {
    let mut decoder = GzDecoder::new(bytes);
    let mut decoded = Vec::new();
    decoder
        .by_ref()
        .take(MAX_ANALYSIS_SOURCE_BYTES as u64 + 1)
        .read_to_end(&mut decoded)?;
    let truncated = decoded.len() > MAX_ANALYSIS_SOURCE_BYTES;
    if truncated {
        decoded.truncate(MAX_ANALYSIS_SOURCE_BYTES);
    }
    Ok((decoded, truncated))
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
