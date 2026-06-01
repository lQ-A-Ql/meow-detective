//! Windows Thumbcache parser.
//!
//! Parses thumbcache_*.db files located at:
//! `C:\Users\{user}\AppData\Local\Microsoft\Windows\Explorer\`
//!
//! These files contain cached thumbnail images for files and folders
//! displayed in Windows Explorer.

use artifacts_core::{
    new_artifact, ArtifactContext, ArtifactExtractor, ArtifactSink, ExtractorReport,
};
use byteorder::{LittleEndian, ReadBytesExt};
use domain::ArtifactFamily;
use std::collections::BTreeMap;
use std::io::{Read, Seek, SeekFrom};

/// Thumbcache file header signatures.
const THUMBCACHE_MAGIC: &[u8; 4] = b"CMMM";
const THUMBCACHE_MAGIC_2: &[u8; 4] = b"ISM1";

/// Thumbcache database parser.
pub struct ThumbcacheExtractor;

impl ArtifactExtractor for ThumbcacheExtractor {
    fn id(&self) -> &'static str {
        "thumbcache"
    }

    fn display_name(&self) -> &'static str {
        "Windows Thumbcache Parser"
    }

    fn family(&self) -> ArtifactFamily {
        ArtifactFamily {
            name: "Thumbcache".into(),
            description: Some("Windows Explorer thumbnail cache (thumbcache_*.db)".into()),
        }
    }

    fn supports_path(&self, file_path: &str) -> bool {
        let lower = file_path.to_lowercase();
        lower.contains("thumbcache") && lower.ends_with(".db")
    }

    fn run(
        &self,
        ctx: ArtifactContext,
        sink: &mut dyn ArtifactSink,
    ) -> Result<ExtractorReport, String> {
        let mut reader = ctx.reader;
        let mut data = Vec::new();
        reader.read_to_end(&mut data).map_err(|e| e.to_string())?;

        if data.len() < 24 {
            return Ok(ExtractorReport {
                artifacts_found: 0,
                timeline_events: 0,
                errors: vec!["File too small for thumbcache header".to_string()],
            });
        }

        // Check magic header
        let magic = &data[0..4];
        if magic != THUMBCACHE_MAGIC && magic != THUMBCACHE_MAGIC_2 {
            return Ok(ExtractorReport {
                artifacts_found: 0,
                timeline_events: 0,
                errors: vec!["Not a valid thumbcache file".to_string()],
            });
        }

        // Parse header
        let mut cursor = std::io::Cursor::new(&data);
        cursor.seek(SeekFrom::Start(4)).map_err(|e| e.to_string())?;

        // Header size (4 bytes)
        let header_size = cursor.read_u32::<LittleEndian>().unwrap_or(0);

        // Version (4 bytes)
        let version = cursor.read_u32::<LittleEndian>().unwrap_or(0);

        // Cache type (4 bytes) - 0x01 = 32x32, 0x02 = 96x96, etc.
        let cache_type = cursor.read_u32::<LittleEndian>().unwrap_or(0);

        // Create artifact
        let mut attrs = BTreeMap::new();
        attrs.insert("file_size".into(), (data.len() as u64).into());
        attrs.insert("header_size".into(), header_size.into());
        attrs.insert("version".into(), version.into());
        attrs.insert("cache_type".into(), cache_type.into());

        // Determine cache type description
        let cache_type_desc = match cache_type {
            0x01 => "32x32",
            0x02 => "96x96",
            0x03 => "256x256",
            0x04 => "1024x1024",
            0x05 => "16x16",
            _ => "Unknown",
        };
        attrs.insert("cache_type_desc".into(), cache_type_desc.into());

        let artifact = new_artifact(
            "Thumbcache",
            format!("Thumbcache: {}", ctx.file_path),
            format!(
                "Windows Explorer thumbnail cache ({}), {} bytes",
                cache_type_desc,
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
    fn thumbcache_supports_path() {
        let extractor = ThumbcacheExtractor;
        assert!(extractor.supports_path(
            "C:/Users/test/AppData/Local/Microsoft/Windows/Explorer/thumbcache_32.db"
        ));
        assert!(extractor.supports_path(
            "C:\\Users\\test\\AppData\\Local\\Microsoft\\Windows\\Explorer\\thumbcache_256.db"
        ));
        assert!(!extractor.supports_path("C:/Windows/System32/config/SYSTEM"));
        assert!(!extractor.supports_path("C:/test/file.txt"));
    }

    #[test]
    fn thumbcache_invalid_data_no_panic() {
        let extractor = ThumbcacheExtractor;
        let ctx = ArtifactContext {
            file_id: domain::FileEntryId("test".to_string()),
            file_path: "thumbcache_32.db".into(),
            reader: Box::new(std::io::Cursor::new(vec![0u8; 100])),
        };
        let mut sink = VecSink::new();
        let report = extractor.run(ctx, &mut sink).unwrap();
        assert_eq!(report.artifacts_found, 0);
        assert!(!report.errors.is_empty());
    }

    #[test]
    fn thumbcache_valid_header() {
        let extractor = ThumbcacheExtractor;
        let mut data = vec![0u8; 1024];
        // Magic header
        data[0..4].copy_from_slice(b"CMMM");
        // Header size (24)
        data[4..8].copy_from_slice(&24u32.to_le_bytes());
        // Version (1)
        data[8..12].copy_from_slice(&1u32.to_le_bytes());
        // Cache type (1 = 32x32)
        data[12..16].copy_from_slice(&1u32.to_le_bytes());

        let ctx = ArtifactContext {
            file_id: domain::FileEntryId("test".to_string()),
            file_path: "thumbcache_32.db".into(),
            reader: Box::new(std::io::Cursor::new(data)),
        };
        let mut sink = VecSink::new();
        let report = extractor.run(ctx, &mut sink).unwrap();
        assert_eq!(report.artifacts_found, 1);
        assert_eq!(sink.artifacts[0].family, "Thumbcache");
    }
}
