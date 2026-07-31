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

#[cfg(test)]
#[path = "../../tests/unit/dto/timeline.rs"]
mod tests;
