//! ABI payload JSON types (design doc §4) and builders.
//!
//! Provenance fields are deliberately absent: the host overwrites them.
//! Attr keys are camelCase, aligned with `base_attrs()` conventions.

use serde::Serialize;
use serde_json::{Map, Value};

#[derive(Serialize)]
pub struct Payload {
    pub artifacts: Vec<PayloadArtifact>,
    #[serde(rename = "timelineEvents")]
    pub timeline_events: Vec<PayloadTimelineEvent>,
    pub warnings: Vec<String>,
}

impl Payload {
    pub fn empty() -> Self {
        Self {
            artifacts: Vec::new(),
            timeline_events: Vec::new(),
            warnings: Vec::new(),
        }
    }

    pub fn to_vec(&self) -> Vec<u8> {
        serde_json::to_vec(self).unwrap_or_else(|_| b"{}".to_vec())
    }

    pub fn artifact(
        &mut self,
        family: &str,
        title: impl Into<String>,
        summary: impl Into<String>,
        attrs: Map<String, Value>,
    ) {
        self.artifacts.push(PayloadArtifact {
            family: family.to_string(),
            title: title.into(),
            summary: summary.into(),
            confidence: Some(0.9),
            attrs,
        });
    }

    pub fn timeline_event(
        &mut self,
        timestamp_utc: String,
        event_type: &str,
        description: impl Into<String>,
        attrs: Map<String, Value>,
    ) {
        self.timeline_events.push(PayloadTimelineEvent {
            timestamp_utc,
            event_type: event_type.to_string(),
            description: description.into(),
            attrs,
        });
    }

    pub fn warn(&mut self, warning: impl Into<String>) {
        self.warnings.push(warning.into());
    }
}

#[derive(Serialize)]
pub struct PayloadArtifact {
    pub family: String,
    pub title: String,
    pub summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f32>,
    pub attrs: Map<String, Value>,
}

#[derive(Serialize)]
pub struct PayloadTimelineEvent {
    #[serde(rename = "timestampUtc")]
    pub timestamp_utc: String,
    #[serde(rename = "eventType")]
    pub event_type: String,
    pub description: String,
    pub attrs: Map<String, Value>,
}

/// Insert a camelCase attr when the value is present and non-empty.
pub fn put_opt(attrs: &mut Map<String, Value>, key: &str, value: Option<&Value>) {
    match value {
        Some(Value::String(text)) if !text.is_empty() => {
            attrs.insert(key.to_string(), Value::String(text.clone()));
        }
        Some(value @ (Value::Number(_) | Value::Bool(_))) => {
            attrs.insert(key.to_string(), value.clone());
        }
        _ => {}
    }
}

pub fn new_attrs() -> Map<String, Value> {
    Map::new()
}
