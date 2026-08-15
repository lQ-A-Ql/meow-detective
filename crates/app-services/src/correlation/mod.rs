pub mod coverage;
mod error;
pub mod graph;
mod helpers;
pub mod rules;
#[cfg(test)]
#[path = "../../tests/unit/correlation/mod.rs"]
mod tests;

use transport::dto::{CorrelationConfidenceDto, CorrelationEdgeKindDto};

pub use error::CorrelationError;

pub use self::graph::{
    get_correlation_snapshot, get_correlation_snapshot_for_case,
    get_correlation_snapshot_incremental, invalidate_correlation_cache,
};

pub(crate) use helpers::{
    artifact_family, confidence_rank, dedup_vec, deleted_preference_score, edge_kind_token,
    first_string_attr, has_family, insert_node, parse_rfc3339_utc, path_suffix_key,
    string_array_attr, CORRELATION_RULE_FAMILIES,
};

pub(crate) const MAX_CORRELATION_ARTIFACTS: usize = 250;
pub(crate) const MAX_CORRELATION_TIMELINE_ROWS: u32 = 250;
pub(crate) const RULE_TIMELINE_CONTEXT_LIMIT: usize = 3;
pub(crate) const RULE_TIMELINE_PROXIMITY_WINDOW_SECS: i64 = 24 * 60 * 60;

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
