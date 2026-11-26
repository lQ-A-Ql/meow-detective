use reports::SummaryReportExporter;
use transport::{
    commands::ReportScopeDto,
    dto::{
        ReportDocumentDto, ReportEntityDto, ReportFormatDto, ReportSourceDto,
        ReportSourceSummaryDto,
    },
};

#[test]
fn structured_export_escapes_html_and_preserves_json_data() {
    let mut summary = ReportSourceSummaryDto::default();
    summary.hardware.record_count = 1;
    summary.hardware.entities.push(ReportEntityDto {
        identity: "device-1".into(),
        fields: [("name".into(), "<script>alert(1)</script>".into())].into(),
        counts: Default::default(),
    });
    let document = ReportDocumentDto {
        case_id: "case-1".into(),
        case_name: "Evidence <review>".into(),
        case_number: None,
        examiner: None,
        generated_at: "2026-11-26T00:00:00Z".into(),
        scope: ReportScopeDto::default(),
        sources: vec![ReportSourceDto {
            data_source_id: "source-1".into(),
            name: "source".into(),
            platform: "windows".into(),
            generation: "analysis-generation".into(),
            hash_status: "unavailable".into(),
            summary_available: true,
            summary,
        }],
    };
    let mut html = Vec::new();
    SummaryReportExporter::export(&mut html, &document, ReportFormatDto::Html).unwrap();
    let html = String::from_utf8(html).unwrap();
    assert!(html.contains("&lt;script&gt;"));
    assert!(!html.contains("<script>"));
    assert!(html.contains("Evidence &lt;review&gt;"));
    let mut json = Vec::new();
    SummaryReportExporter::export(&mut json, &document, ReportFormatDto::Json).unwrap();
    let value: serde_json::Value = serde_json::from_slice(&json).unwrap();
    assert_eq!(
        value["sources"][0]["summary"]["hardware"]["entities"][0]["fields"]["name"],
        "<script>alert(1)</script>"
    );
}
