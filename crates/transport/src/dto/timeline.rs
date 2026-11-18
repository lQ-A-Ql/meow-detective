use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimelineEventDto {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data_source_id: Option<String>,
    pub source_object_id: String,
    pub event_type: String,
    pub ts: String,
    pub title: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parser_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parser_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_attribution: Option<String>,
    pub attrs: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TimelineFacetCountDto {
    pub value: String,
    pub count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TimelineHistogramBucketDto {
    pub start_ts: String,
    pub end_ts: String,
    pub count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TimelineFacetsDto {
    pub total_events: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_ts: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_ts: Option<String>,
    pub event_types: Vec<TimelineFacetCountDto>,
    pub data_sources: Vec<TimelineFacetCountDto>,
    pub histogram: Vec<TimelineHistogramBucketDto>,
}

/// A cluster of timeline events sharing the same `event_type` and `description`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimelineClusterDto {
    pub event_type: String,
    pub description: String,
    pub count: u64,
    pub first_ts: String,
    pub last_ts: String,
    pub sample_event_ids: Vec<String>,
}

/// All clusters for a given `event_type`, together with the total event count for that type.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimelineStripeDto {
    pub clusters: Vec<TimelineClusterDto>,
    pub total_events: u64,
}

/// Server-side aggregated timeline view. The outer map is keyed by `event_type`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimelineAggregatedDto {
    pub stripes_by_type: HashMap<String, TimelineStripeDto>,
}

/// Readiness of one source's timeline projection marker. A missing marker means
/// `complete` for legacy-case compatibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TimelineProjectionStateDto {
    Complete,
    Building,
    Deferred,
}

/// Per-source timeline projection status returned by the status command.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TimelineProjectionSourceStatusDto {
    pub data_source_id: String,
    pub state: TimelineProjectionStateDto,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub identity: Option<String>,
    pub inserted_events: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub available_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

/// Case-wide timeline projection status; `state` is the worst per-source state
/// (deferred > building > complete).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TimelineProjectionStatusDto {
    pub state: TimelineProjectionStateDto,
    pub sources: Vec<TimelineProjectionSourceStatusDto>,
}

#[cfg(test)]
#[path = "../../tests/unit/dto/timeline.rs"]
mod tests;
