use std::io::{self, Write};

pub struct CsvExporter;

impl CsvExporter {
    pub fn export_rows(
        writer: &mut impl Write,
        headers: &[&str],
        rows: &[Vec<String>],
    ) -> io::Result<()> {
        writeln!(writer, "{}", headers.join(","))?;
        for row in rows {
            let escaped = row
                .iter()
                .map(|cell| format!("\"{}\"", sanitize_csv_cell(cell).replace('"', "\"\"")))
                .collect::<Vec<_>>();
            writeln!(writer, "{}", escaped.join(","))?;
        }
        Ok(())
    }
}

fn sanitize_csv_cell(value: &str) -> String {
    if value.starts_with(['=', '+', '-', '@']) {
        format!("\t{value}")
    } else {
        value.to_owned()
    }
}

#[cfg(test)]
#[path = "../../tests/unit/csv/exporter.rs"]
mod tests;
