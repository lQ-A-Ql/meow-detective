use super::*;

#[test]
fn event_topic_serializes_as_wire_topic() {
    let json = serde_json::to_string(&EventTopic::JobProgress).unwrap();
    assert_eq!(json, "\"job-progress\"");

    let search = serde_json::to_string(&EventTopic::SearchIndexProgress).unwrap();
    assert_eq!(search, "\"search-index-progress\"");

    let imported = serde_json::to_string(&EventTopic::DataSourceImported).unwrap();
    assert_eq!(imported, "\"data-source-imported\"");

    let phase = serde_json::to_string(&EventTopic::ImportPhaseProgress).unwrap();
    assert_eq!(phase, "\"import-phase-progress\"");

    let partial = serde_json::to_string(&EventTopic::ImportPartialResult).unwrap();
    assert_eq!(partial, "\"import-partial-result\"");

    let cancellation = serde_json::to_string(&EventTopic::JobCancellation).unwrap();
    assert_eq!(cancellation, "\"job-cancellation\"");

    let cache = serde_json::to_string(&EventTopic::CacheIndexStatus).unwrap();
    assert_eq!(cache, "\"cache-index-status\"");

    let report = serde_json::to_string(&EventTopic::PerformanceReportReady).unwrap();
    assert_eq!(report, "\"performance-report-ready\"");
    let extraction = serde_json::to_string(&EventTopic::AnalysisExtractionProgress).unwrap();
    assert_eq!(extraction, "\"analysis-extraction-progress\"");
}

#[test]
fn runtime_event_topics_are_tauri_safe() {
    let topics = [
        TOPIC_IMPORT_PHASE_PROGRESS,
        TOPIC_IMPORT_PARTIAL_RESULT,
        TOPIC_JOB_CANCELLATION,
        TOPIC_CACHE_INDEX_STATUS,
        TOPIC_PERFORMANCE_REPORT_READY,
        TOPIC_ANALYSIS_EXTRACTION_PROGRESS,
    ];

    for topic in topics {
        assert!(!topic.contains('.'), "{topic} must not contain dots");
        assert!(
            topic
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '/' | ':' | '_')),
            "{topic} contains a character rejected by Tauri event names"
        );
    }
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
    assert_eq!(
        EventTopic::AnalysisExtractionProgress.as_str(),
        TOPIC_ANALYSIS_EXTRACTION_PROGRESS
    );
}

#[test]
fn unknown_event_topic_is_rejected() {
    let err = serde_json::from_str::<EventTopic>("\"unknown-topic\"").unwrap_err();
    assert!(err.to_string().contains("unknown variant"));
}
