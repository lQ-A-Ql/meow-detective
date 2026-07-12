use std::io::{self, Write};

pub struct JsonExporter;

impl JsonExporter {
    pub fn export(writer: &mut impl Write, value: &serde_json::Value) -> io::Result<()> {
        write!(
            writer,
            "{}",
            serde_json::to_string_pretty(value).unwrap_or_default()
        )?;
        Ok(())
    }
}

#[cfg(test)]
#[path = "../../tests/unit/json/exporter.rs"]
mod tests;
