//! Windows SRU (System Resource Usage) database parser.
//!
//! Parses the SRUDB.DAT file located at:
//! `C:\Windows\System32\sru\SRUDB.DAT`
//!
//! This is a SQLite database containing system resource usage data including:
//! - Application resource usage
//! - Network usage statistics
//! - Energy usage data
//! - Push notification data

use artifacts_core::{
    new_artifact, ArtifactContext, ArtifactExtractor, ArtifactSink,
    ExtractorReport,
};
use domain::ArtifactFamily;
use std::collections::BTreeMap;
use std::io::Read;

/// SRU database parser.
pub struct SruExtractor;

impl ArtifactExtractor for SruExtractor {
    fn id(&self) -> &'static str {
        "sru"
    }

    fn display_name(&self) -> &'static str {
        "Windows SRU Database Parser"
    }

    fn family(&self) -> ArtifactFamily {
        ArtifactFamily {
            name: "SRU".into(),
            description: Some("Windows System Resource Usage database (SRUDB.DAT)".into()),
        }
    }

    fn supports_path(&self, file_path: &str) -> bool {
        let lower = file_path.to_lowercase();
        lower.ends_with("srudb.dat")
    }

    fn run(
        &self,
        ctx: ArtifactContext,
        sink: &mut dyn ArtifactSink,
    ) -> Result<ExtractorReport, String> {
        // Read the entire file to check if it's a valid SQLite database
        let mut reader = ctx.reader;
        let mut data = Vec::new();
        reader.read_to_end(&mut data).map_err(|e| e.to_string())?;

        // Check SQLite magic header
        if data.len() < 16 || &data[0..16] != b"SQLite format 3\0" {
            return Ok(ExtractorReport {
                artifacts_found: 0,
                timeline_events: 0,
                errors: vec!["Not a valid SQLite database".to_string()],
            });
        }

        // Create a generic SRU artifact since we can't easily parse SQLite
        // without a full SQLite library in the reader context
        let mut attrs = BTreeMap::new();
        attrs.insert("file_size".into(), (data.len() as u64).into());
        attrs.insert("format".into(), "SRUDB.DAT".into());
        attrs.insert("database_type".into(), "System Resource Usage".into());

        // Try to extract some basic info from the header
        if data.len() >= 100 {
            // SQLite page size is at offset 16-17
            let page_size = u16::from_be_bytes([data[16], data[17]]);
            let effective_page_size = if page_size == 1 { 65536u32 } else { page_size as u32 };
            attrs.insert("page_size".into(), effective_page_size.into());
        }

        let artifact = new_artifact(
            "SRU",
            format!("SRU Database: {}", ctx.file_path),
            format!("Windows System Resource Usage database, {} bytes", data.len()),
            Some(&ctx.file_id),
            attrs,
        );
        sink.write_artifact(artifact);

        Ok(ExtractorReport {
            artifacts_found: 1,
            timeline_events: 0,
            errors: vec![],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use artifacts_core::VecSink;

    #[test]
    fn sru_supports_path() {
        let extractor = SruExtractor;
        assert!(extractor.supports_path("C:/Windows/System32/sru/SRUDB.DAT"));
        assert!(extractor.supports_path("C:\\Windows\\System32\\sru\\SRUDB.DAT"));
        assert!(!extractor.supports_path("C:/Windows/System32/config/SYSTEM"));
    }

    #[test]
    fn sru_invalid_data_no_panic() {
        let extractor = SruExtractor;
        let ctx = ArtifactContext {
            file_id: domain::FileEntryId("test".to_string()),
            file_path: "SRUDB.DAT".into(),
            reader: Box::new(std::io::Cursor::new(vec![0u8; 100])),
        };
        let mut sink = VecSink::new();
        let report = extractor.run(ctx, &mut sink).unwrap();
        assert_eq!(report.artifacts_found, 0);
        assert!(!report.errors.is_empty());
    }

    #[test]
    fn sru_valid_header() {
        let extractor = SruExtractor;
        let mut data = vec![0u8; 1024];
        // SQLite header
        data[0..16].copy_from_slice(b"SQLite format 3\0");
        // Page size (4096)
        data[16] = 0x10;
        data[17] = 0x00;

        let ctx = ArtifactContext {
            file_id: domain::FileEntryId("test".to_string()),
            file_path: "SRUDB.DAT".into(),
            reader: Box::new(std::io::Cursor::new(data)),
        };
        let mut sink = VecSink::new();
        let report = extractor.run(ctx, &mut sink).unwrap();
        assert_eq!(report.artifacts_found, 1);
    }
}
