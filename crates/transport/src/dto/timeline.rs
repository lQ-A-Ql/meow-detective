use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimelineEventDto {
    pub id: String,
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
mod tests {
    use super::*;

    #[test]
    fn timeline_event_dto_serializes_optional_provenance_as_camel_case() {
        let dto = TimelineEventDto {
            id: "timeline-1".to_string(),
            source_object_id: "file-1".to_string(),
            event_type: "FILE_MODIFIED".to_string(),
            ts: "2026-06-04T00:00:00Z".to_string(),
            title: "Modified".to_string(),
            description: "description".to_string(),
            parser_id: Some("timeline.macb".to_string()),
            parser_version: Some("1.0.0".to_string()),
            confidence: Some(0.8),
            source_attribution: Some("$STANDARD_INFORMATION".to_string()),
            attrs: BTreeMap::new(),
        };

        let value = serde_json::to_value(dto).unwrap();

        assert_eq!(value["sourceObjectId"], "file-1");
        assert_eq!(value["parserId"], "timeline.macb");
        assert_eq!(value["parserVersion"], "1.0.0");
        assert!((value["confidence"].as_f64().unwrap() - 0.8).abs() < 0.000001);
        assert_eq!(value["sourceAttribution"], "$STANDARD_INFORMATION");
        assert!(value.get("parser_id").is_none());
        assert!(value.get("source_attribution").is_none());
    }

    #[test]
    fn timeline_event_dto_skips_missing_optional_provenance() {
        let dto = TimelineEventDto {
            id: "timeline-1".to_string(),
            source_object_id: "file-1".to_string(),
            event_type: "FILE_MODIFIED".to_string(),
            ts: "2026-06-04T00:00:00Z".to_string(),
            title: "Modified".to_string(),
            description: "description".to_string(),
            parser_id: None,
            parser_version: None,
            confidence: None,
            source_attribution: None,
            attrs: BTreeMap::new(),
        };

        let value = serde_json::to_value(dto).unwrap();

        assert!(value.get("parserId").is_none());
        assert!(value.get("parserVersion").is_none());
        assert!(value.get("confidence").is_none());
        assert!(value.get("sourceAttribution").is_none());
    }
}
