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
#[path = "../../tests/unit/csv/exporter.rs"]
mod tests;
