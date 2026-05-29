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
}
