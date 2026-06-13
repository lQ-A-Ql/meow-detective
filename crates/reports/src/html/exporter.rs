use domain::CaseMeta;
use infrastructure::text::html_escape;
use std::io::{self, Write};

pub struct HtmlReportExporter;

#[derive(Debug, Clone, Default)]
pub struct HtmlCorrelationLeadSection {
    pub title: String,
    pub confidence: String,
    pub families: Vec<String>,
    pub primary_file_id: String,
    pub summary: String,
    pub supporting_node_ids: Vec<String>,
    pub match_signals: Vec<String>,
    pub provenance: Vec<String>,
    pub caveats: Vec<String>,
}

impl HtmlReportExporter {
    pub fn export(
        writer: &mut impl Write,
        case: &CaseMeta,
        files: &[String],
        artifacts: &[String],
    ) -> io::Result<()> {
        Self::export_with_sections(writer, case, files, artifacts, &[], &[])
    }

    pub fn export_with_analysis(
        writer: &mut impl Write,
        case: &CaseMeta,
        files: &[String],
        artifacts: &[String],
        analysis: &[String],
    ) -> io::Result<()> {
        Self::export_with_sections(writer, case, files, artifacts, analysis, &[])
    }

    pub fn export_with_sections(
        writer: &mut impl Write,
        case: &CaseMeta,
        files: &[String],
        artifacts: &[String],
        analysis: &[String],
        correlation: &[String],
    ) -> io::Result<()> {
        Self::export_with_structured_sections(
            writer,
            case,
            files,
            artifacts,
            analysis,
            &[],
            correlation,
            &[],
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn export_with_structured_sections(
        writer: &mut impl Write,
        case: &CaseMeta,
        files: &[String],
        artifacts: &[String],
        analysis: &[String],
        governance_rows: &[String],
        correlation_rows: &[String],
        correlation_leads: &[HtmlCorrelationLeadSection],
    ) -> io::Result<()> {
        writeln!(writer, "<!DOCTYPE html><html><head><meta charset=\"UTF-8\"><title>Forensic Report - {}</title>", html_escape(&case.name))?;
        writeln!(writer, "<style>body{{font-family:sans-serif;margin:20px;}}h1{{color:#333;}}table{{border-collapse:collapse;width:100%%;}}th,td{{border:1px solid #ccc;padding:6px;text-align:left;vertical-align:top;}}th{{background:#eee;}}.lead-card{{border:1px solid #ccc;padding:12px;margin:0 0 12px 0;border-radius:6px;background:#fafafa;}}.lead-meta{{display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:8px;margin:8px 0;}}.lead-block{{margin-top:8px;}}.lead-block h3{{font-size:12px;margin:0 0 4px 0;color:#555;text-transform:uppercase;letter-spacing:.04em;}}ul{{margin:4px 0 0 18px;padding:0;}}</style>")?;
        writeln!(writer, "</head><body>")?;
        writeln!(
            writer,
            "<h1>Forensic Report: {}</h1>",
            html_escape(&case.name)
        )?;
        writeln!(
            writer,
            "<p>Case #: {} | Examiner: {} | Generated: {}</p>",
            html_escape(case.number.as_deref().unwrap_or("N/A")),
            html_escape(case.examiner.as_deref().unwrap_or("Unknown")),
            chrono::Utc::now().to_rfc3339()
        )?;

        writeln!(
            writer,
            "<h2>Evidence Files</h2><table><tr><th>Path</th></tr>"
        )?;
        for f in files {
            writeln!(writer, "<tr><td>{}</td></tr>", html_escape(f))?;
        }
        writeln!(writer, "</table>")?;

        writeln!(
            writer,
            "<h2>Artifacts</h2><table><tr><th>Artifact</th></tr>"
        )?;
        for a in artifacts {
            writeln!(writer, "<tr><td>{}</td></tr>", html_escape(a))?;
        }

        writeln!(
            writer,
            "</table><h2>Analysis Provenance</h2><table><tr><th>Status</th></tr>"
        )?;
        for item in analysis {
            writeln!(writer, "<tr><td>{}</td></tr>", html_escape(item))?;
        }

        writeln!(
            writer,
            "</table><h2>Governance Snapshot</h2><table><tr><th>Signal</th></tr>"
        )?;
        for item in governance_rows {
            writeln!(writer, "<tr><td>{}</td></tr>", html_escape(item))?;
        }

        writeln!(
            writer,
            "</table><h2>Correlation Leads</h2><table><tr><th>Lead</th></tr>"
        )?;
        for item in correlation_rows {
            writeln!(writer, "<tr><td>{}</td></tr>", html_escape(item))?;
        }
        writeln!(writer, "</table>")?;

        if !correlation_leads.is_empty() {
            writeln!(writer, "<h2>Correlation Lead Details</h2>")?;
            for lead in correlation_leads {
                writeln!(writer, "<section class=\"lead-card\">")?;
                writeln!(writer, "<h3>{}</h3>", html_escape(&lead.title))?;
                writeln!(writer, "<p>{}</p>", html_escape(&lead.summary))?;
                writeln!(
                    writer,
                    "<div class=\"lead-meta\"><div><strong>Confidence</strong><br/>{}</div><div><strong>Primary File</strong><br/>{}</div></div>",
                    html_escape(&lead.confidence),
                    html_escape(&lead.primary_file_id)
                )?;

                if !lead.families.is_empty() {
                    writeln!(
                        writer,
                        "<div class=\"lead-block\"><h3>Rule Families</h3><ul>"
                    )?;
                    for item in &lead.families {
                        writeln!(writer, "<li>{}</li>", html_escape(item))?;
                    }
                    writeln!(writer, "</ul></div>")?;
                }

                if !lead.supporting_node_ids.is_empty() {
                    writeln!(
                        writer,
                        "<div class=\"lead-block\"><h3>Supporting Nodes</h3><ul>"
                    )?;
                    for item in &lead.supporting_node_ids {
                        writeln!(writer, "<li>{}</li>", html_escape(item))?;
                    }
                    writeln!(writer, "</ul></div>")?;
                }

                if !lead.match_signals.is_empty() {
                    writeln!(
                        writer,
                        "<div class=\"lead-block\"><h3>Match Signals</h3><ul>"
                    )?;
                    for item in &lead.match_signals {
                        writeln!(writer, "<li>{}</li>", html_escape(item))?;
                    }
                    writeln!(writer, "</ul></div>")?;
                }

                if !lead.provenance.is_empty() {
                    writeln!(writer, "<div class=\"lead-block\"><h3>Provenance</h3><ul>")?;
                    for item in &lead.provenance {
                        writeln!(writer, "<li>{}</li>", html_escape(item))?;
                    }
                    writeln!(writer, "</ul></div>")?;
                }

                if !lead.caveats.is_empty() {
                    writeln!(writer, "<div class=\"lead-block\"><h3>Caveats</h3><ul>")?;
                    for item in &lead.caveats {
                        writeln!(writer, "<li>{}</li>", html_escape(item))?;
                    }
                    writeln!(writer, "</ul></div>")?;
                }

                writeln!(writer, "</section>")?;
            }
        }

        writeln!(writer, "</body></html>")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
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
}
