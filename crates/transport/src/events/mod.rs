use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub const TOPIC_CASE_OPENED: &str = "case-opened";
pub const TOPIC_CASE_CLOSED: &str = "case-closed";
pub const TOPIC_JOB_CREATED: &str = "job-created";
pub const TOPIC_JOB_STARTED: &str = "job-started";
pub const TOPIC_JOB_PROGRESS: &str = "job-progress";
pub const TOPIC_JOB_COMPLETED: &str = "job-completed";
pub const TOPIC_JOB_FAILED: &str = "job-failed";
pub const TOPIC_JOB_CANCELLED: &str = "job-cancelled";
pub const TOPIC_DATA_SOURCE_IMPORTED: &str = "data-source-imported";
pub const TOPIC_ARTIFACT_ADDED: &str = "artifact-added";
pub const TOPIC_TIMELINE_UPDATED: &str = "timeline-updated";
pub const TOPIC_SEARCH_INDEX_PROGRESS: &str = "search-index_progress";
pub const TOPIC_PARTITION_PROGRESS: &str = "partition-progress";
pub const TOPIC_IMPORT_PHASE_PROGRESS: &str = "import.phase_progress";
pub const TOPIC_IMPORT_PARTIAL_RESULT: &str = "import.partial_result";
pub const TOPIC_JOB_CANCELLATION: &str = "job.cancellation";
pub const TOPIC_CACHE_INDEX_STATUS: &str = "cache.index_status";
pub const TOPIC_PERFORMANCE_REPORT_READY: &str = "performance.report_ready";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EventTopic {
    CaseOpened,
    CaseClosed,
    JobCreated,
    JobStarted,
    JobProgress,
    JobCompleted,
    JobFailed,
    JobCancelled,
    DataSourceImported,
    ArtifactAdded,
    TimelineUpdated,
    #[serde(rename = "search-index_progress")]
    SearchIndexProgress,
    PartitionProgress,
    #[serde(rename = "import.phase_progress")]
    ImportPhaseProgress,
    #[serde(rename = "import.partial_result")]
    ImportPartialResult,
    #[serde(rename = "job.cancellation")]
    JobCancellation,
    #[serde(rename = "cache.index_status")]
    CacheIndexStatus,
    #[serde(rename = "performance.report_ready")]
    PerformanceReportReady,
}

impl EventTopic {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CaseOpened => TOPIC_CASE_OPENED,
            Self::CaseClosed => TOPIC_CASE_CLOSED,
            Self::JobCreated => TOPIC_JOB_CREATED,
            Self::JobStarted => TOPIC_JOB_STARTED,
            Self::JobProgress => TOPIC_JOB_PROGRESS,
            Self::JobCompleted => TOPIC_JOB_COMPLETED,
            Self::JobFailed => TOPIC_JOB_FAILED,
            Self::JobCancelled => TOPIC_JOB_CANCELLED,
            Self::DataSourceImported => TOPIC_DATA_SOURCE_IMPORTED,
            Self::ArtifactAdded => TOPIC_ARTIFACT_ADDED,
            Self::TimelineUpdated => TOPIC_TIMELINE_UPDATED,
            Self::SearchIndexProgress => TOPIC_SEARCH_INDEX_PROGRESS,
            Self::PartitionProgress => TOPIC_PARTITION_PROGRESS,
            Self::ImportPhaseProgress => TOPIC_IMPORT_PHASE_PROGRESS,
            Self::ImportPartialResult => TOPIC_IMPORT_PARTIAL_RESULT,
            Self::JobCancellation => TOPIC_JOB_CANCELLATION,
            Self::CacheIndexStatus => TOPIC_CACHE_INDEX_STATUS,
            Self::PerformanceReportReady => TOPIC_PERFORMANCE_REPORT_READY,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventEnvelope<T> {
    pub event_id: String,
    pub topic: EventTopic,
    pub ts: DateTime<Utc>,
    pub payload: T,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_topic_serializes_as_wire_topic() {
        let json = serde_json::to_string(&EventTopic::JobProgress).unwrap();
        assert_eq!(json, "\"job-progress\"");

        let search = serde_json::to_string(&EventTopic::SearchIndexProgress).unwrap();
        assert_eq!(search, "\"search-index_progress\"");

        let imported = serde_json::to_string(&EventTopic::DataSourceImported).unwrap();
        assert_eq!(imported, "\"data-source-imported\"");

        let phase = serde_json::to_string(&EventTopic::ImportPhaseProgress).unwrap();
        assert_eq!(phase, "\"import.phase_progress\"");

        let partial = serde_json::to_string(&EventTopic::ImportPartialResult).unwrap();
        assert_eq!(partial, "\"import.partial_result\"");

        let cancellation = serde_json::to_string(&EventTopic::JobCancellation).unwrap();
        assert_eq!(cancellation, "\"job.cancellation\"");

        let cache = serde_json::to_string(&EventTopic::CacheIndexStatus).unwrap();
        assert_eq!(cache, "\"cache.index_status\"");

        let report = serde_json::to_string(&EventTopic::PerformanceReportReady).unwrap();
        assert_eq!(report, "\"performance.report_ready\"");
    }

    #[test]
    fn new_event_topics_match_constant_strings() {
        assert_eq!(
            EventTopic::ImportPhaseProgress.as_str(),
            TOPIC_IMPORT_PHASE_PROGRESS
        );
        assert_eq!(
            EventTopic::ImportPartialResult.as_str(),
            TOPIC_IMPORT_PARTIAL_RESULT
        );
        assert_eq!(EventTopic::JobCancellation.as_str(), TOPIC_JOB_CANCELLATION);
        assert_eq!(
            EventTopic::CacheIndexStatus.as_str(),
            TOPIC_CACHE_INDEX_STATUS
        );
        assert_eq!(
            EventTopic::PerformanceReportReady.as_str(),
            TOPIC_PERFORMANCE_REPORT_READY
        );
    }

    #[test]
    fn unknown_event_topic_is_rejected() {
        let err = serde_json::from_str::<EventTopic>("\"unknown-topic\"").unwrap_err();
        assert!(err.to_string().contains("unknown variant"));
    }
}
