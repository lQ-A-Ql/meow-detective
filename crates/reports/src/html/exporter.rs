use domain::CaseMeta;
use infrastructure::text::html_escape;
use std::io::{self, Write};

pub struct HtmlReportExporter;

impl HtmlReportExporter {
    pub fn export(
        writer: &mut impl Write,
        case: &CaseMeta,
        files: &[String],
        artifacts: &[String],
    ) -> io::Result<()> {
        Self::export_with_analysis(writer, case, files, artifacts, &[])
    }

    pub fn export_with_analysis(
        writer: &mut impl Write,
        case: &CaseMeta,
        files: &[String],
        artifacts: &[String],
        analysis: &[String],
    ) -> io::Result<()> {
        writeln!(writer, "<!DOCTYPE html><html><head><meta charset=\"UTF-8\"><title>Forensic Report - {}</title>", html_escape(&case.name))?;
        writeln!(writer, "<style>body{{font-family:sans-serif;margin:20px;}}h1{{color:#333;}}table{{border-collapse:collapse;width:100%%;}}th,td{{border:1px solid #ccc;padding:6px;text-align:left;}}th{{background:#eee;}}</style>")?;
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
        writeln!(writer, "</table></body></html>")?;
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
}
