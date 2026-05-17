use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct TimelineEventId(pub String);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineEvent {
    pub id: TimelineEventId,
    pub source_object_id: String,
    pub event_type: String,
    pub timestamp: DateTime<Utc>,
    pub title: String,
    pub description: String,
    pub attrs: BTreeMap<String, Value>,
}
