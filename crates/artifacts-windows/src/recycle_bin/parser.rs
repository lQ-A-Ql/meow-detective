//! Windows Recycle Bin ($I file) parser.
//!
//! Supports both v1 (Windows Vista/7: 4-byte header) and v2
//! (Windows 8+: 8-byte header) formats with automatic detection.

use artifacts_core::{
    new_artifact, new_timeline_event, ArtifactContext, ArtifactExtractor, ArtifactSink,
    ExtractorReport,
};
use chrono::{DateTime, Datelike, TimeZone, Utc};
use domain::ArtifactFamily;
use std::collections::BTreeMap;
use std::io::Read;

pub struct RecycleBinExtractor;

struct ParsedRecycleInfo {
    file_size: u64,
    deletion_time: Option<DateTime<Utc>>,
    original_path: Option<String>,
}

impl RecycleBinExtractor {
    fn filetime_to_dt(ft: u64) -> Option<DateTime<Utc>> {
        if ft == 0 || ft >= 0x8000000000000000 {
            return None;
        }
        let secs = (ft / 10_000_000) as i64 - 11_644_473_600;
        Utc.timestamp_opt(secs, ((ft % 10_000_000) * 100) as u32)
            .single()
    }

    fn parse_i_file<R: Read>(reader: &mut R) -> Result<ParsedRecycleInfo, String> {
        // Read enough bytes to detect v1 vs v2 format
        let mut header = vec![0u8; 24];
        let n = reader.read(&mut header).map_err(|e| e.to_string())?;
        if n < 16 {
            return Err("$I file too short".to_string());
        }

        // Try v2 first (Windows 8+): 8-byte header_size, 8-byte file_size, 8-byte FILETIME
        let v2_header_size = u64::from_le_bytes(header[0..8].try_into().unwrap());
        // v1 format (Vista/7): 4-byte header_size, 4-byte file_size, 8-byte FILETIME
        let v1_header_size = u32::from_le_bytes(header[0..4].try_into().unwrap()) as u64;

        // Detect format: v2 headers are typically 0x18 (24) or larger; v1 headers are ≤ 512
        let (header_size, file_size, deletion_ft) = if (24..=1024).contains(&v2_header_size) {
            let fs = u64::from_le_bytes(header[8..16].try_into().unwrap());
            let ft = u64::from_le_bytes(header[16..24].try_into().unwrap());
            (v2_header_size, fs, ft)
        } else {
            // v1: header is [4B header_size][4B file_size][8B FILETIME]
            let fs = u32::from_le_bytes(header[4..8].try_into().unwrap());
            let _ft_start = if n >= 16 {
                8
            } else {
                return Err("$I file truncated".to_string());
            };
            let mut ft_bytes = [0u8; 8];
            if n >= 16 {
                ft_bytes.copy_from_slice(&header[8..16]);
            } else {
                return Err("$I file too short for FILETIME".to_string());
            }
            let ft = u64::from_le_bytes(ft_bytes);
            (v1_header_size, fs as u64, ft)
        };
        let deletion_time = Self::filetime_to_dt(deletion_ft);

        let path = if header_size > 0 && header_size <= 2048 {
            let header_bytes = if (24..=1024).contains(&v2_header_size) {
                24
            } else {
                16
            };
            let padding = header_size.saturating_sub(header_bytes) as usize;
            // Seek past any remaining padding
            if padding > 0 && n < header_size as usize {
                let mut skip = vec![0u8; padding.min(1024)];
                reader
                    .read_exact(&mut skip)
                    .map_err(|e| format!("skip padding: {e}"))?;
            } else if n > header_size as usize {
                // We already read past the header into path data; use remaining bytes
            }
            // Read path (up to 520 bytes, UTF-16LE null-terminated)
            let mut raw = vec![0u8; 520];
            let path_n = reader
                .read(&mut raw)
                .map_err(|e| format!("read path: {e}"))?;
            raw.truncate(path_n);
            let chars: Vec<u16> = raw
                .chunks_exact(2)
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .collect();
            let s = String::from_utf16(&chars).unwrap_or_default();
            let s = s.trim_end_matches('\0').to_string();
            if s.is_empty() {
                None
            } else {
                Some(s)
            }
        } else {
            None
        };

        Ok(ParsedRecycleInfo {
            file_size,
            deletion_time,
            original_path: path,
        })
    }
}

impl ArtifactExtractor for RecycleBinExtractor {
    fn id(&self) -> &'static str {
        "recycle_bin"
    }
    fn display_name(&self) -> &'static str {
        "Windows Recycle Bin Parser"
    }
    fn family(&self) -> ArtifactFamily {
        ArtifactFamily {
            name: "RecycleBin".into(),
            description: Some("Windows Recycle Bin ($I/$R)".into()),
        }
    }
    fn supports_path(&self, file_path: &str) -> bool {
        file_path.contains("$Recycle.Bin") && file_path.contains("$I")
    }

    fn run(
        &self,
        ctx: ArtifactContext,
        sink: &mut dyn ArtifactSink,
    ) -> Result<ExtractorReport, String> {
        let mut reader = ctx.reader;
        let parsed = Self::parse_i_file(&mut reader)?;
        let file_size = parsed.file_size;
        let deletion_time = parsed.deletion_time;
        let original_path = parsed.original_path;

        let mut attrs = BTreeMap::new();
        attrs.insert(
            "recovered_file_size".into(),
            serde_json::Value::Number(file_size.into()),
        );
        if let Some(ref path) = original_path {
            attrs.insert(
                "original_path".into(),
                serde_json::Value::String(path.clone()),
            );
        }

        let summary = match (&original_path, deletion_time) {
            (Some(p), Some(t)) => format!("Deleted: {} at {}", p, t.to_rfc3339()),
            (Some(p), None) => format!("Deleted: {}", p),
            (None, _) => format!("Recycled file, {} bytes", file_size),
        };

        let artifact = new_artifact(
            "RecycleBin",
            "Recycle Bin: deleted file".to_string(),
            summary,
            Some(&ctx.file_id),
            attrs,
        );
        sink.write_artifact(artifact);

        let mut events = 0u32;
        if let Some(dt) = deletion_time {
            if dt.year() > 2000 {
                let mut tl_attrs = BTreeMap::new();
                if let Some(ref p) = original_path {
                    tl_attrs.insert("path".into(), serde_json::Value::String(p.clone()));
                }
                let ev = new_timeline_event(
                    &ctx.file_id,
                    "FILE_DELETED",
                    dt,
                    "File deleted".into(),
                    format!("File deleted at {}", dt.to_rfc3339()),
                    tl_attrs,
                );
                sink.write_timeline_event(ev);
                events += 1;
            }
        }

        Ok(ExtractorReport {
            artifacts_found: 1,
            timeline_events: events,
            errors: vec![],
        })
    }
}
