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
        writeln!(writer, "</table></body></html>")?;
        Ok(())
    }
}
