use std::io::{self, Write};

/// Sanitize a CSV cell value to prevent formula injection.
///
/// Prefixes cells starting with =, +, -, @ with a tab character
/// to prevent spreadsheet applications from interpreting them as formulas.
fn sanitize_csv_cell(s: &str) -> String {
    if let Some(first) = s.chars().next() {
        if matches!(first, '=' | '+' | '-' | '@') {
            return format!("\t{}", s);
        }
    }
    s.to_string()
}

pub struct CsvExporter;

impl CsvExporter {
    pub fn export_artifacts(
        writer: &mut impl Write,
        headers: &[&str],
        rows: &[Vec<String>],
    ) -> io::Result<()> {
        writeln!(writer, "{}", headers.join(","))?;
        for row in rows {
            let escaped: Vec<String> = row
                .iter()
                .map(|c| {
                    let sanitized = sanitize_csv_cell(c);
                    format!("\"{}\"", sanitized.replace('"', "\"\""))
                })
                .collect();
            writeln!(writer, "{}", escaped.join(","))?;
        }
        Ok(())
    }

    /// Export correlation leads as CSV rows.
    ///
    /// Each row must have exactly 9 columns:
    /// `lead_id`, `title`, `confidence`, `families`, `primary_file_path`,
    /// `supporting_node_count`, `match_signals_count`, `provenance_sources`, `caveats`
    pub fn export_correlation_leads(
        writer: &mut impl Write,
        rows: &[Vec<String>],
    ) -> io::Result<()> {
        let headers = [
            "lead_id",
            "title",
            "confidence",
            "families",
            "primary_file_path",
            "supporting_node_count",
            "match_signals_count",
            "provenance_sources",
            "caveats",
        ];
        Self::export_artifacts(writer, &headers, rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_csv_cell_normal() {
        assert_eq!(sanitize_csv_cell("hello"), "hello");
    }

    #[test]
    fn test_sanitize_csv_cell_formula() {
        assert_eq!(sanitize_csv_cell("=SUM(A1:A2)"), "\t=SUM(A1:A2)");
        assert_eq!(sanitize_csv_cell("+cmd"), "\t+cmd");
        assert_eq!(sanitize_csv_cell("-DANGEROUS"), "\t-DANGEROUS");
        assert_eq!(sanitize_csv_cell("@SUM"), "\t@SUM");
    }

    #[test]
    fn test_export_artifacts() {
        let mut output = Vec::new();
        let headers = vec!["Name", "Value"];
        let rows = vec![
            vec!["test".to_string(), "123".to_string()],
            vec!["hello".to_string(), "world".to_string()],
        ];

        CsvExporter::export_artifacts(&mut output, &headers, &rows).unwrap();
        let result = String::from_utf8(output).unwrap();
        assert!(result.contains("Name,Value"));
        assert!(result.contains("\"test\""));
    }

    #[test]
    fn test_export_correlation_leads() {
        let mut output = Vec::new();
        let rows = vec![
            vec![
                "lead:1".to_string(),
                "cmd.exe 形成关联线索".to_string(),
                "direct".to_string(),
                "LNK; Prefetch".to_string(),
                "file-1".to_string(),
                "2".to_string(),
                "3".to_string(),
                "artifact:artifact-1:LNK; timeline:timeline-1:FILE_MODIFIED".to_string(),
                "路径类匹配仍需回跳原始工件复核。".to_string(),
            ],
            vec![
                "lead:2".to_string(),
                "svchost.exe 网络活动线索".to_string(),
                "strong".to_string(),
                "EVTX".to_string(),
                "file-2".to_string(),
                "0".to_string(),
                "1".to_string(),
                "".to_string(),
                "".to_string(),
            ],
        ];

        CsvExporter::export_correlation_leads(&mut output, &rows).unwrap();
        let result = String::from_utf8(output).unwrap();

        assert!(result.contains("lead_id,title,confidence,families,primary_file_path,supporting_node_count,match_signals_count,provenance_sources,caveats"));
        assert!(result.contains("\"lead:1\""));
        assert!(result.contains("\"cmd.exe 形成关联线索\""));
        assert!(result.contains("\"direct\""));
        assert!(result.contains("\"LNK; Prefetch\""));
        assert!(result.contains("\"file-1\""));
        assert!(result.contains("\"2\""));
        assert!(result.contains("\"3\""));
        assert!(result.contains("artifact:artifact-1:LNK; timeline:timeline-1:FILE_MODIFIED"));
        assert!(result.contains("路径类匹配仍需回跳原始工件复核。"));
        assert!(result.contains("\"lead:2\""));
        assert!(result.contains("\"strong\""));
        // Empty provenance and caveats should still appear as quoted empty strings
        assert!(result.contains("\"\""));
    }

    #[test]
    fn test_export_correlation_leads_sanitizes_formula() {
        let mut output = Vec::new();
        let rows = vec![vec![
            "=LEAD_ID".to_string(),
            "=CMD.EXE".to_string(),
            "direct".to_string(),
            "-DANGEROUS".to_string(),
            "+file-1".to_string(),
            "0".to_string(),
            "0".to_string(),
            "@SUM".to_string(),
            "".to_string(),
        ]];

        CsvExporter::export_correlation_leads(&mut output, &rows).unwrap();
        let result = String::from_utf8(output).unwrap();

        assert!(result.contains("\"\t=LEAD_ID\""));
        assert!(result.contains("\"\t=CMD.EXE\""));
        assert!(result.contains("\"\t-DANGEROUS\""));
        assert!(result.contains("\"\t+file-1\""));
        assert!(result.contains("\"\t@SUM\""));
    }
}
