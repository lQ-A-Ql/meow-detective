use std::io::{self, Write};

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
                .map(|c| format!("\"{}\"", c.replace('"', "\"\"")))
                .collect();
            writeln!(writer, "{}", escaped.join(","))?;
        }
        Ok(())
    }
}
