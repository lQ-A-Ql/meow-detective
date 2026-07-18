use super::*;

#[test]
fn extraction_run_serializes_section_progress_as_camel_case() {
    let dto = AnalysisExtractionRunDto {
        status: AnalysisParseStatusDto::Partial,
        scanned_count: 3,
        checkpoint_hit_count: 2,
        artifact_count: 9,
        timeline_event_count: 4,
        sections: vec![AnalysisExtractionSectionRunDto {
            key: "LinuxJournal".to_string(),
            label: "Linux 日志".to_string(),
            status: AnalysisParseStatusDto::Parsed,
            scanned_count: 2,
            artifact_count: 7,
            timeline_event_count: 4,
            warnings: vec!["rotated log truncated".to_string()],
        }],
        generated_at: "2026-07-10T00:00:00Z".to_string(),
        warnings: vec!["overall warning".to_string()],
    };

    let value = serde_json::to_value(dto).unwrap();

    assert_eq!(value["scannedCount"], 3);
    assert_eq!(value["checkpointHitCount"], 2);
    assert_eq!(value["artifactCount"], 9);
    assert_eq!(value["timelineEventCount"], 4);
    assert_eq!(value["sections"][0]["key"], "LinuxJournal");
    assert_eq!(value["sections"][0]["scannedCount"], 2);
    assert_eq!(value["sections"][0]["timelineEventCount"], 4);
    assert!(value.get("scanned_count").is_none());
    assert!(value["sections"][0].get("scanned_count").is_none());
}
