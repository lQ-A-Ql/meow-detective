//! Windows SRU (System Resource Usage) database recognizer.
//!
//! Recognizes the SRUDB.DAT file located at:
//! `C:\Windows\System32\sru\SRUDB.DAT`
//!
//! Modern Windows SRUDB.DAT files are ESE/Jet Blue databases. This extractor
//! is intentionally fail-closed: it records bounded file-level metadata only
//! for ESE-looking SRUDB.DAT inputs and refuses SQLite or unknown content rather
//! than mislabeling unrelated databases as SRU records.

use artifacts_core::{
    new_artifact, ArtifactContext, ArtifactExtractor, ArtifactSink, ExtractorReport,
};
use domain::ArtifactFamily;
use std::collections::BTreeMap;
use std::io::Read;

/// SRU database parser.
pub struct SruExtractor;

impl SruExtractor {
    fn looks_like_sqlite(data: &[u8]) -> bool {
        data.len() >= 16 && &data[0..16] == b"SQLite format 3\0"
    }

    fn looks_like_ese(data: &[u8]) -> bool {
        data.len() >= 8 && u32::from_le_bytes([data[4], data[5], data[6], data[7]]) == 0x89AB_CDEF
    }
}

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

        // SRUDB.DAT is expected to be an ESE/Jet Blue database. Do not accept
        // the SQLite header here: doing so falsely routes arbitrary SQLite
        // databases named SRUDB.DAT into the Windows SRU artifact family.
        if Self::looks_like_sqlite(&data) {
            return Ok(ExtractorReport {
                artifacts_found: 0,
                timeline_events: 0,
                errors: vec!["SRUDB.DAT uses ESE/Jet Blue format, not SQLite".to_string()],
            });
        }
        if !Self::looks_like_ese(&data) {
            return Ok(ExtractorReport {
                artifacts_found: 0,
                timeline_events: 0,
                errors: vec!["Not a recognized SRU ESE database".to_string()],
            });
        }

        // Create a generic SRU artifact until full ESE table parsing is added.
        let mut attrs = BTreeMap::new();
        attrs.insert("file_size".into(), (data.len() as u64).into());
        attrs.insert("format".into(), "ESE/Jet Blue".into());
        attrs.insert("database_type".into(), "System Resource Usage".into());

        // ESE page size is not represented by the SQLite header field; leave
        // table/schema details to a future ESE parser.

        let artifact = new_artifact(
            "SRU",
            format!("SRU Database: {}", ctx.file_path),
            format!(
                "Windows System Resource Usage database, {} bytes",
                data.len()
            ),
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
        assert_eq!(report.errors, vec!["Not a recognized SRU ESE database"]);
    }

    #[test]
    fn sru_sqlite_header_is_rejected() {
        let extractor = SruExtractor;
        let mut data = vec![0u8; 1024];
        data[0..16].copy_from_slice(b"SQLite format 3\0");

        let ctx = ArtifactContext {
            file_id: domain::FileEntryId("test".to_string()),
            file_path: "SRUDB.DAT".into(),
            reader: Box::new(std::io::Cursor::new(data)),
        };
        let mut sink = VecSink::new();
        let report = extractor.run(ctx, &mut sink).unwrap();
        assert_eq!(report.artifacts_found, 0);
        assert_eq!(
            report.errors,
            vec!["SRUDB.DAT uses ESE/Jet Blue format, not SQLite"]
        );
        assert!(sink.artifacts.is_empty());
    }

    #[test]
    fn sru_ese_header_creates_file_level_artifact() {
        let extractor = SruExtractor;
        let mut data = vec![0u8; 1024];
        data[4..8].copy_from_slice(&0x89AB_CDEFu32.to_le_bytes());

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
