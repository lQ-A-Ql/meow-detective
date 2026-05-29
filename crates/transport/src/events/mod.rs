use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub const TOPIC_CASE_OPENED: &str = "case-opened";
pub const TOPIC_CASE_CLOSED: &str = "case-closed";
pub const TOPIC_JOB_CREATED: &str = "job-created";
pub const TOPIC_JOB_STARTED: &str = "job-started";
pub const TOPIC_JOB_PROGRESS: &str = "job-progress";
pub const TOPIC_JOB_COMPLETED: &str = "job-completed";
pub const TOPIC_JOB_FAILED: &str = "job-failed";
pub const TOPIC_ARTIFACT_ADDED: &str = "artifact-added";
pub const TOPIC_TIMELINE_UPDATED: &str = "timeline-updated";
pub const TOPIC_SEARCH_INDEX_PROGRESS: &str = "search-index_progress";
pub const TOPIC_PARTITION_PROGRESS: &str = "partition-progress";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventEnvelope<T> {
    pub event_id: String,
    pub topic: String,
    pub ts: DateTime<Utc>,
    pub payload: T,
}
