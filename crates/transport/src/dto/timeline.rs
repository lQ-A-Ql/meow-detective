use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimelineEventDto {
    pub id: String,
    pub source_object_id: String,
    pub event_type: String,
    pub ts: String,
    pub title: String,
    pub description: String,
    pub attrs: BTreeMap<String, Value>,
}
