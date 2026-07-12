use super::*;
use domain::{CaseId, CaseMeta};

#[test]
fn analysis_provenance_is_html_escaped() {
    let case = CaseMeta {
        id: CaseId("case".to_string()),
        name: "<case>".to_string(),
        number: None,
        examiner: None,
        notes: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    let mut output = Vec::new();

    HtmlReportExporter::export_with_analysis(
        &mut output,
        &case,
        &[],
        &[],
        &["registry.system <script>alert(1)</script>".to_string()],
    )
    .unwrap();

    let html = String::from_utf8(output).unwrap();
    assert!(html.contains("Analysis Provenance"));
    assert!(html.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
    assert!(!html.contains("<script>alert(1)</script>"));
}

#[test]
fn correlation_rows_are_html_escaped() {
    let case = CaseMeta {
        id: CaseId("case".to_string()),
        name: "case".to_string(),
        number: None,
        examiner: None,
        notes: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    let mut output = Vec::new();

    HtmlReportExporter::export_with_sections(
        &mut output,
        &case,
        &[],
        &[],
        &[],
        &["lead <b>cmd.exe</b>".to_string()],
    )
    .unwrap();

    let html = String::from_utf8(output).unwrap();
    assert!(html.contains("Correlation Leads"));
    assert!(html.contains("lead &lt;b&gt;cmd.exe&lt;/b&gt;"));
    assert!(!html.contains("<b>cmd.exe</b>"));
}

#[test]
fn structured_correlation_sections_are_html_escaped() {
    let case = CaseMeta {
        id: CaseId("case".to_string()),
        name: "case".to_string(),
        number: None,
        examiner: None,
        notes: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    let mut output = Vec::new();

    HtmlReportExporter::export_with_structured_sections(
        &mut output,
        &case,
        &[],
        &[],
        &[],
        &[],
        &["summary row".to_string()],
        &[HtmlCorrelationLeadSection {
            title: "lead <b>1</b>".to_string(),
            confidence: "direct".to_string(),
            families: vec!["LNK<script>".to_string()],
            primary_file_id: "file-1".to_string(),
            summary: "summary <script>1</script>".to_string(),
            supporting_node_ids: vec!["artifact:<x>".to_string()],
            match_signals: vec!["signal <tag>".to_string()],
            provenance: vec!["artifact:1:LNK:bestEffort".to_string()],
            caveats: vec!["caveat <warn>".to_string()],
        }],
    )
    .unwrap();

    let html = String::from_utf8(output).unwrap();
    assert!(html.contains("Correlation Lead Details"));
    assert!(html.contains("lead &lt;b&gt;1&lt;/b&gt;"));
    assert!(html.contains("summary &lt;script&gt;1&lt;/script&gt;"));
    assert!(html.contains("LNK&lt;script&gt;"));
    assert!(html.contains("signal &lt;tag&gt;"));
    assert!(!html.contains("<script>1</script>"));
}

#[test]
fn governance_rows_are_html_escaped() {
    let case = CaseMeta {
        id: CaseId("case".to_string()),
        name: "case".to_string(),
        number: None,
        examiner: None,
        notes: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    let mut output = Vec::new();

    HtmlReportExporter::export_with_structured_sections(
        &mut output,
        &case,
        &[],
        &[],
        &[],
        &["gate <b>security-baseline</b>".to_string()],
        &[],
        &[],
    )
    .unwrap();

    let html = String::from_utf8(output).unwrap();
    assert!(html.contains("Governance Snapshot"));
    assert!(html.contains("gate &lt;b&gt;security-baseline&lt;/b&gt;"));
    assert!(!html.contains("<b>security-baseline</b>"));
}
