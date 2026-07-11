use super::source_artifact_row;

use std::collections::BTreeMap;

use transport::dto::ArtifactRowDto;

fn cross_source_artifact() -> ArtifactRowDto {
    ArtifactRowDto {
        id: "ds:windows-report:artifact-cross-source".to_string(),
        artifact_type: "Prefetch".to_string(),
        title: "Cross-source artifact".to_string(),
        summary: "invalid source attribution fixture".to_string(),
        source_object_id: Some("ds:linux-report:file-1".to_string()),
        extractor_id: Some("prefetch".to_string()),
        extractor_version: Some("1.0.0".to_string()),
        confidence: Some(1.0),
        source_attribution: Some("test fixture".to_string()),
        created_at: "2026-07-11T00:00:00Z".to_string(),
        attrs: BTreeMap::new(),
    }
}

fn assert_cross_source_rejection(error: crate::report::ReportError) {
    assert!(
        error
            .to_string()
            .contains("report record crosses data source boundaries"),
        "unexpected report error: {error}"
    );
}

#[test]
fn json_export_rejects_cross_source_artifact_reference() {
    let error = crate::report::json_records::source_artifact(&cross_source_artifact())
        .expect_err("JSON export must reject cross-source artifact references");

    assert_cross_source_rejection(error);
}

#[test]
fn csv_export_rejects_cross_source_artifact_reference() {
    let error = source_artifact_row(&cross_source_artifact())
        .expect_err("CSV export must reject cross-source artifact references");

    assert_cross_source_rejection(error);
}

#[test]
fn html_export_rejects_cross_source_artifact_reference() {
    let error = crate::report::html::format_artifact_dto_report_row(&cross_source_artifact())
        .expect_err("HTML export must reject cross-source artifact references");

    assert_cross_source_rejection(error);
}
