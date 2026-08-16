//! ABI payload JSON types (design doc §4) and builders.
//!
//! Provenance fields are deliberately absent: the host overwrites them.
//! Attr keys are camelCase, aligned with `base_attrs()` conventions.
//!
//! This plugin emits artifacts and warnings only: the request carries no
//! file timestamps, so there is no time source for timeline events and
//! `timesAreLocal` is pinned to `false`.

use serde::Serialize;
use serde_json::{Map, Value};

#[derive(Serialize)]
pub struct Payload {
    pub artifacts: Vec<PayloadArtifact>,
    #[serde(rename = "timelineEvents")]
    pub timeline_events: Vec<PayloadTimelineEvent>,
    pub warnings: Vec<String>,
    /// No timestamp source exists in this plugin version; nothing local is
    /// ever emitted, so the flag stays `false`.
    #[serde(rename = "timesAreLocal")]
    pub times_are_local: bool,
}

impl Payload {
    pub fn empty() -> Self {
        Self {
            artifacts: Vec::new(),
            timeline_events: Vec::new(),
            warnings: Vec::new(),
            times_are_local: false,
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

/// Kept for schema stability (`timelineEvents` must always serialize, even
/// when empty); this plugin version never constructs one.
#[derive(Serialize)]
pub struct PayloadTimelineEvent {
    #[serde(rename = "timestampUtc")]
    pub timestamp_utc: String,
    #[serde(rename = "eventType")]
    pub event_type: String,
    pub description: String,
    pub attrs: Map<String, Value>,
}

pub fn new_attrs() -> Map<String, Value> {
    Map::new()
}
