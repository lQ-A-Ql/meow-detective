//! Windows SRU (System Resource Usage) database recognizer.
//!
//! Recognizes the SRUDB.DAT file located at:
//! `C:\Windows\System32\sru\SRUDB.DAT`
//!
//! Modern Windows SRUDB.DAT files are ESE/Jet Blue databases. This extractor
//! validates the ESE magic byte and extracts available database-header metadata
//! (page size, page count, database state) without implementing a full ESE
//! table parser.

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

    /// Attempt to extract ESE database-header metadata.
    ///
    /// ESE database page size is stored at a version-dependent offset. This
    /// probes several well-known offsets and accepts the first candidate that
    /// looks like a valid page size (power of two, 2 KiB – 64 KiB).
    fn extract_ese_metadata(data: &[u8]) -> BTreeMap<String, serde_json::Value> {
        let mut attrs = BTreeMap::new();

        // Need at least the first 48 bytes for header probing.
        if data.len() < 48 {
            return attrs;
        }

        // Database format version at offset 8 (4-byte LE).
        let ver = u32::from_le_bytes([data[8], data[9], data[10], data[11]]);
        attrs.insert("ese_db_version".into(), ver.into());

        // Probe candidate offsets for the page-size field.
        // Valid ESE page sizes are powers of two in [0x800 .. 0x10000].
        for &off in &[0x20usize, 0x40, 0x48] {
            if off + 4 <= data.len() {
                let ps =
                    u32::from_le_bytes([data[off], data[off + 1], data[off + 2], data[off + 3]]);
                if (0x800..=0x10000).contains(&ps) && ps.is_power_of_two() {
                    attrs.insert("page_size".into(), ps.into());
                    let np = data.len() as u64 / ps as u64;
                    if np > 0 {
                        attrs.insert("estimated_pages".into(), (np).into());
                    }
                    break;
                }
            }
        }

        // Probe candidate offsets for the database state field.
        // Known clean values: 1–5 (JET_dbstateJustCreated … JET_dbstateForceDetach).
        for &off in &[0x28usize, 0x30] {
            if off + 4 <= data.len() {
                let st =
                    u32::from_le_bytes([data[off], data[off + 1], data[off + 2], data[off + 3]]);
                if (1..=5).contains(&st) {
                    let desc = match st {
                        1 => "JustCreated",
                        2 => "DirtyShutdown",
                        3 => "CleanShutdown",
                        4 => "BeingConverted",
                        5 => "ForceDetach",
                        _ => "Unknown",
                    };
                    attrs.insert("database_state".into(), st.into());
                    attrs.insert("database_state_desc".into(), desc.into());
                    break;
                }
            }
        }

        // Fallback: we know it is an ESE database even if we couldn't parse details.
        if !attrs.contains_key("page_size") {
            attrs.insert("ese_detected".into(), true.into());
        }

        attrs
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

        let mut attrs = BTreeMap::new();
        attrs.insert("file_size".into(), (data.len() as u64).into());
        attrs.insert("format".into(), "ESE/Jet Blue".into());
        attrs.insert("database_type".into(), "System Resource Usage".into());

        // Enrich with any metadata readable from the ESE database header.
        let ese_meta = Self::extract_ese_metadata(&data);
        attrs.extend(ese_meta);

        let page_info = match (attrs.get("page_size"), attrs.get("estimated_pages")) {
            (Some(ps), Some(np)) => {
                format!(
                    ", {} pages of {} bytes each",
                    np.as_u64().unwrap_or(0),
                    ps.as_u64().unwrap_or(0)
                )
            }
            _ => String::new(),
        };

        let artifact = new_artifact(
            "SRU",
            format!("SRU Database: {}", ctx.file_path),
            format!(
                "Windows System Resource Usage database, {} bytes{}",
                data.len(),
                page_info
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

    #[test]
    fn sru_ese_header_extracts_page_size_and_state() {
        let extractor = SruExtractor;
        // Build an ESE header with a plausible page-size at offset 0x40
        // and a CleanShutdown state at offset 0x28.
        let mut data = vec![0u8; 8192];
        data[4..8].copy_from_slice(&0x89AB_CDEFu32.to_le_bytes()); // magic
        data[8..12].copy_from_slice(&0x0620u32.to_le_bytes()); // version
                                                               // page size 4096 at offset 0x40
        data[0x40..0x44].copy_from_slice(&4096u32.to_le_bytes());
        // database state 3 (CleanShutdown) at offset 0x28
        data[0x28..0x2C].copy_from_slice(&3u32.to_le_bytes());

        let ctx = ArtifactContext {
            file_id: domain::FileEntryId("test".to_string()),
            file_path: "SRUDB.DAT".into(),
            reader: Box::new(std::io::Cursor::new(data)),
        };
        let mut sink = VecSink::new();
        let report = extractor.run(ctx, &mut sink).unwrap();
        assert_eq!(report.artifacts_found, 1);
        let attrs = &sink.artifacts[0].attrs;
        assert_eq!(attrs.get("page_size").and_then(|v| v.as_u64()), Some(4096));
        assert_eq!(
            attrs.get("estimated_pages").and_then(|v| v.as_u64()),
            Some(2)
        );
        assert_eq!(
            attrs.get("database_state").and_then(|v| v.as_u64()),
            Some(3)
        );
        assert_eq!(
            attrs.get("database_state_desc").and_then(|v| v.as_str()),
            Some("CleanShutdown")
        );
    }
}
