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
}
