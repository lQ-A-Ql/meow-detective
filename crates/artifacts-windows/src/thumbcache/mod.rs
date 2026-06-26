//! Windows Thumbcache parser.
//!
//! Parses thumbcache_*.db files located at:
//! `C:\Users\{user}\AppData\Local\Microsoft\Windows\Explorer\`
//!
//! These files contain cached thumbnail images for files and folders
//! displayed in Windows Explorer.
//!
//! The parser reads the cache header and enumerates entry-level metadata
//! (hash, data size) without extracting the actual thumbnail image data.

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

/// Maximum entries to enumerate (safety cap).
const MAX_ENTRIES: u32 = 10_000;

/// Thumbcache database parser.
pub struct ThumbcacheExtractor;

impl ThumbcacheExtractor {
    /// Enumerate cache entries starting at `entry_offset`.
    ///
    /// Each entry has: 4-byte entry size, 8-byte hash, 4-byte data size,
    /// then padding/thumbnail data.
    fn enumerate_entries(
        cursor: &mut std::io::Cursor<&[u8]>,
        entry_offset: u64,
        data_len: usize,
    ) -> (u32, u64, Vec<serde_json::Value>) {
        let mut entries: Vec<serde_json::Value> = Vec::new();
        let mut count: u32 = 0;
        let mut total_data: u64 = 0;

        if cursor.seek(SeekFrom::Start(entry_offset)).is_err() {
            return (0, 0, entries);
        }

        while count < MAX_ENTRIES {
            let base = cursor.position() as usize;
            if base + 16 > data_len {
                break;
            }

            let entry_size = match cursor.read_u32::<LittleEndian>() {
                Ok(s) if s >= 16 => s as u64,
                _ => break,
            };

            let hash_hi = match cursor.read_u32::<LittleEndian>() {
                Ok(v) => v,
                _ => break,
            };
            let hash_lo = match cursor.read_u32::<LittleEndian>() {
                Ok(v) => v,
                _ => break,
            };

            let data_size = match cursor.read_u32::<LittleEndian>() {
                Ok(s) => s,
                _ => break,
            };

            count += 1;
            total_data += data_size as u64;

            // Record first 100 entry summaries for the artifact.
            if entries.len() < 100 {
                let mut info = serde_json::Map::new();
                info.insert(
                    "hash".into(),
                    serde_json::Value::String(format!("{:08X}{:08X}", hash_hi, hash_lo)),
                );
                info.insert("data_size".into(), serde_json::Value::from(data_size));
                entries.push(serde_json::Value::Object(info));
            }

            // Advance to the next entry (entry_size includes the header we just read).
            let skip = entry_size.saturating_sub(16);
            if skip == 0 {
                break;
            }
            let next_abs = base as u64 + entry_size;
            if cursor.seek(SeekFrom::Start(next_abs)).is_err() {
                break;
            }
        }

        (count, total_data, entries)
    }
}

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
        let mut cursor = std::io::Cursor::new(data.as_slice());
        cursor.seek(SeekFrom::Start(4)).map_err(|e| e.to_string())?;

        // Header size (4 bytes)
        let header_size = cursor.read_u32::<LittleEndian>().unwrap_or(0);

        // Version (4 bytes)
        let version = cursor.read_u32::<LittleEndian>().unwrap_or(0);

        // Cache type (4 bytes) - 0x01 = 32x32, 0x02 = 96x96, etc.
        let cache_type = cursor.read_u32::<LittleEndian>().unwrap_or(0);

        // Offset to first cache entry (bytes 16-19 of the header).
        cursor
            .seek(SeekFrom::Start(16))
            .map_err(|e| e.to_string())?;
        let entry_offset = cursor.read_u32::<LittleEndian>().unwrap_or(0);

        // Enumerate cache entries if the offset is valid.
        let (entry_count, total_data_size, entry_list) =
            if entry_offset >= 24 && (entry_offset as usize) < data.len() {
                Self::enumerate_entries(&mut cursor, entry_offset as u64, data.len())
            } else {
                (0, 0, Vec::new())
            };

        // Determine cache type description
        let cache_type_desc = match cache_type {
            0x01 => "32x32",
            0x02 => "96x96",
            0x03 => "256x256",
            0x04 => "1024x1024",
            0x05 => "16x16",
            _ => "Unknown",
        };

        // Create artifact
        let mut attrs = BTreeMap::new();
        attrs.insert("file_size".into(), (data.len() as u64).into());
        attrs.insert("header_size".into(), header_size.into());
        attrs.insert("version".into(), version.into());
        attrs.insert("cache_type".into(), cache_type.into());
        attrs.insert("cache_type_desc".into(), cache_type_desc.into());

        if entry_count > 0 {
            attrs.insert("entry_count".into(), entry_count.into());
            attrs.insert("total_thumbnail_data_size".into(), total_data_size.into());
            attrs.insert("entries".into(), serde_json::Value::Array(entry_list));
        }

        let summary = match entry_count {
            0 => format!(
                "Windows Explorer thumbnail cache ({}), {} bytes",
                cache_type_desc,
                data.len()
            ),
            n => format!(
                "Windows Explorer thumbnail cache ({}), {} entries, {} bytes ({} bytes thumbnail data)",
                cache_type_desc,
                n,
                data.len(),
                total_data_size
            ),
        };

        let artifact = new_artifact(
            "Thumbcache",
            format!("Thumbcache: {}", ctx.file_path),
            summary,
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

    #[test]
    fn thumbcache_enumerates_entries() {
        let extractor = ThumbcacheExtractor;
        let entry_offset = 24u32;
        // Build two entries starting at offset 24.
        // Entry 1: size=40 (16 hdr + 24 data), hash=0xDEADBEEF_CAFEBABE, data_size=24
        // Entry 2: size=36 (16 hdr + 20 data), hash=0x12345678_9ABCDEF0, data_size=20
        let mut data = vec![0u8; 100];
        data[0..4].copy_from_slice(b"CMMM");
        data[4..8].copy_from_slice(&24u32.to_le_bytes()); // header_size
        data[8..12].copy_from_slice(&2u32.to_le_bytes()); // version
        data[12..16].copy_from_slice(&1u32.to_le_bytes()); // cache_type
        data[16..20].copy_from_slice(&entry_offset.to_le_bytes());

        // Entry 1 at offset 24
        let off = entry_offset as usize;
        data[off..off + 4].copy_from_slice(&40u32.to_le_bytes()); // entry size
        data[off + 4..off + 8].copy_from_slice(&0xCAFEBABEu32.to_le_bytes()); // hash hi
        data[off + 8..off + 12].copy_from_slice(&0xDEADBEEFu32.to_le_bytes()); // hash lo
        data[off + 12..off + 16].copy_from_slice(&24u32.to_le_bytes()); // data size

        // Entry 2 at offset 64 (24 + 40)
        let off2 = off + 40;
        data[off2..off2 + 4].copy_from_slice(&36u32.to_le_bytes()); // entry size
        data[off2 + 4..off2 + 8].copy_from_slice(&0x9ABCDEF0u32.to_le_bytes()); // hash hi
        data[off2 + 8..off2 + 12].copy_from_slice(&0x12345678u32.to_le_bytes()); // hash lo
        data[off2 + 12..off2 + 16].copy_from_slice(&20u32.to_le_bytes()); // data size

        let ctx = ArtifactContext {
            file_id: domain::FileEntryId("tc-entries".to_string()),
            file_path: "thumbcache_32.db".into(),
            reader: Box::new(std::io::Cursor::new(data)),
        };
        let mut sink = VecSink::new();
        let report = extractor.run(ctx, &mut sink).unwrap();
        assert_eq!(report.artifacts_found, 1);
        assert!(report.errors.is_empty());

        let attrs = &sink.artifacts[0].attrs;
        assert_eq!(attrs.get("entry_count").and_then(|v| v.as_u64()), Some(2));
        assert_eq!(
            attrs
                .get("total_thumbnail_data_size")
                .and_then(|v| v.as_u64()),
            Some(44)
        );

        let entries = attrs.get("entries").and_then(|v| v.as_array());
        assert!(entries.is_some());
        let entries = entries.unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0]["hash"], "CAFEBABEDEADBEEF");
        assert_eq!(entries[0]["data_size"], 24);
        assert_eq!(entries[1]["hash"], "9ABCDEF012345678");
        assert_eq!(entries[1]["data_size"], 20);
    }
}
