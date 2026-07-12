pub mod coverage;
mod error;
pub mod graph;
pub mod rules;
#[cfg(test)]
#[path = "../../tests/unit/correlation/mod.rs"]
mod tests;

use chrono::Utc;
use serde_json::Value;
use std::collections::BTreeMap;
use transport::dto::{CorrelationConfidenceDto, CorrelationEdgeKindDto, CorrelationNodeDto};

pub use error::CorrelationError;

pub use self::graph::{
    get_correlation_snapshot, get_correlation_snapshot_for_case,
    get_correlation_snapshot_incremental, invalidate_correlation_cache,
};

pub(crate) const MAX_CORRELATION_ARTIFACTS: usize = 250;
pub(crate) const MAX_CORRELATION_TIMELINE_ROWS: u32 = 250;
pub(crate) const RULE_TIMELINE_CONTEXT_LIMIT: usize = 3;
pub(crate) const RULE_TIMELINE_PROXIMITY_WINDOW_SECS: i64 = 24 * 60 * 60;
pub(crate) const CORRELATION_RULE_FAMILIES: [(&str, &str); 8] = [
    ("LNK", "LNK"),
    ("Prefetch", "Prefetch"),
    ("Registry", "Registry"),
    ("RecycleBin", "Recycle Bin"),
    ("BrowserDownload", "Browser Download"),
    ("BrowserHistory", "Browser History"),
    ("EmailMessage", "Email"),
    ("JumpList", "JumpList"),
];

#[derive(Debug, Default)]
pub(crate) struct CorrelationSourceGroup {
    pub source_object_id: String,
    pub file: Option<domain::FileEntry>,
    pub artifacts: Vec<transport::dto::ArtifactRowDto>,
    pub timelines: Vec<transport::dto::TimelineEventDto>,
}

#[derive(Debug, Clone)]
pub(crate) struct CorrelationRuleMatch {
    pub artifact: transport::dto::ArtifactRowDto,
    pub file: domain::FileEntry,
    pub kind: CorrelationEdgeKindDto,
    pub confidence: CorrelationConfidenceDto,
    pub summary: String,
    pub caveat: String,
}

#[derive(Debug, Clone)]
pub(crate) struct CorrelationRuleGroup {
    pub file: domain::FileEntry,
    pub matches: Vec<CorrelationRuleMatch>,
    pub timelines: Vec<transport::dto::TimelineEventDto>,
    pub timeline_signals: Vec<String>,
}

// ── Helpers used across modules ──

pub(crate) fn confidence_rank(confidence: &CorrelationConfidenceDto) -> u8 {
    match confidence {
        CorrelationConfidenceDto::Direct => 4,
        CorrelationConfidenceDto::Strong => 3,
        CorrelationConfidenceDto::Weak => 2,
        CorrelationConfidenceDto::Heuristic => 1,
    }
}

pub(crate) fn dedup_vec<T>(values: &mut Vec<T>)
where
    T: Clone + PartialEq,
{
    let mut deduped = Vec::new();
    for item in values.iter().cloned() {
        if !deduped.contains(&item) {
            deduped.push(item);
        }
    }
    *values = deduped;
}

pub(crate) fn edge_kind_token(kind: &CorrelationEdgeKindDto) -> &'static str {
    match kind {
        CorrelationEdgeKindDto::SourceReference => "source",
        CorrelationEdgeKindDto::SharedSourceObject => "shared",
        CorrelationEdgeKindDto::TemporalContext => "temporal",
        CorrelationEdgeKindDto::PathMatch => "path",
        CorrelationEdgeKindDto::NameMatch => "name",
        CorrelationEdgeKindDto::RecoveredOriginalPath => "recovered",
    }
}

pub(crate) fn has_family(families: &[String], family: &str) -> bool {
    families
        .iter()
        .any(|item| item.eq_ignore_ascii_case(family))
}

pub(crate) fn artifact_family(artifact_type: &str) -> Option<String> {
    if artifact_type.eq_ignore_ascii_case("RegistryValue") || artifact_type.starts_with("Registry")
    {
        return Some("Registry".to_string());
    }
    CORRELATION_RULE_FAMILIES
        .iter()
        .find(|(family, _)| family.eq_ignore_ascii_case(artifact_type))
        .map(|(family, _)| (*family).to_string())
}

pub(crate) fn insert_node(
    map: &mut BTreeMap<String, CorrelationNodeDto>,
    node: CorrelationNodeDto,
) {
    match map.get_mut(&node.id) {
        Some(existing) => {
            existing.related_count = existing.related_count.max(node.related_count);
            existing.badges.extend(node.badges);
            existing.jumps.extend(node.jumps);
            dedup_vec(&mut existing.badges);
            dedup_vec(&mut existing.jumps);
            if existing.subtitle.is_none() {
                existing.subtitle = node.subtitle;
            }
            if existing.source_object_id.is_none() {
                existing.source_object_id = node.source_object_id;
            }
        }
        None => {
            map.insert(node.id.clone(), node);
        }
    }
}

// ── Attribute access helpers ──

pub(crate) fn first_string_attr(attrs: &BTreeMap<String, Value>, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| attrs.get(*key))
        .and_then(|value| value.as_str())
        .map(|value| value.to_string())
}

pub(crate) fn string_array_attr(attrs: &BTreeMap<String, Value>, key: &str) -> Vec<String> {
    attrs
        .get(key)
        .and_then(|value| value.as_array())
        .map(|values| {
            values
                .iter()
                .filter_map(|value| value.as_str().map(|item| item.to_string()))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

pub(crate) fn parse_rfc3339_utc(value: &str) -> Option<chrono::DateTime<Utc>> {
    chrono::DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|item| item.with_timezone(&Utc))
}

pub(crate) fn path_suffix_key(value: &str) -> String {
    let normalized = rules::normalize_path(value);
    let bytes = normalized.as_bytes();
    if normalized.len() >= 3 && bytes[1] == b':' && bytes[2] == b'/' {
        normalized[3..].to_string()
    } else {
        normalized.trim_start_matches('/').to_string()
    }
}

pub(crate) fn deleted_preference_score(
    file: &domain::FileEntry,
    prefer_deleted: Option<bool>,
) -> u8 {
    match prefer_deleted {
        Some(expected) if file.deleted == expected => 0,
        Some(_) => 1,
        None => 0,
    }
}
