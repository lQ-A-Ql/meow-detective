//! Windows Prefetch v30 parser.
//! Format reference: libscca / Windows Internals.
//! v30 (Win10+): SCCA magic at 0x0000, format version at 0x0004,
//! file_size at 0x0008, executable name at 0x0010 (60 bytes UTF-16LE),
//! hash at 0x004C, run_count at end of file information section.

use artifacts_core::{
    new_artifact, new_timeline_event, ArtifactContext, ArtifactExtractor, ArtifactSink,
    ExtractorReport,
};
use byteorder::{LittleEndian, ReadBytesExt};
use chrono::{DateTime, Datelike, TimeZone, Utc};
use domain::ArtifactFamily;
use std::collections::BTreeMap;
use std::io::Read;

pub struct PrefetchExtractor;

impl PrefetchExtractor {
    fn filetime_to_dt(ft: u64) -> Option<DateTime<Utc>> {
        if ft == 0 || ft >= 0x8000000000000000 {
            return None;
        }
        let secs = (ft / 10_000_000) as i64 - 11_644_473_600;
        Utc.timestamp_opt(secs, ((ft % 10_000_000) * 100) as u32)
            .single()
    }

    fn read_utf16le_string<R: Read>(reader: &mut R, byte_len: usize) -> Option<String> {
        let mut buf = vec![0u8; byte_len.min(256)];
        reader.read_exact(&mut buf).ok()?;
        let end = buf.iter().position(|&b| {
            b == 0
                && buf
                    .get(buf.iter().position(|&x| x == b).unwrap_or(0) + 1)
                    .copied()
                    .unwrap_or(1)
                    == 0
        });
        let end = end.unwrap_or(buf.len());
        let chars: Vec<u16> = buf[..end]
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        String::from_utf16(&chars).ok()
    }
}

impl ArtifactExtractor for PrefetchExtractor {
    fn id(&self) -> &'static str {
        "prefetch"
    }
    fn display_name(&self) -> &'static str {
        "Windows Prefetch Parser (v30)"
    }
    fn family(&self) -> ArtifactFamily {
        ArtifactFamily {
            name: "Prefetch".into(),
            description: Some("Windows Prefetch files (.pf) v30".into()),
        }
    }
    fn supports_path(&self, file_path: &str) -> bool {
        file_path.to_lowercase().ends_with(".pf")
    }

    fn run(
        &self,
        ctx: ArtifactContext,
        sink: &mut dyn ArtifactSink,
    ) -> Result<ExtractorReport, String> {
        let mut reader = ctx.reader;
        let mut magic = [0u8; 4];
        reader.read_exact(&mut magic).map_err(|e| e.to_string())?;
        if &magic != b"SCCA" && &magic != b"MAM\x04" {
            return Ok(ExtractorReport {
                artifacts_found: 0,
                timeline_events: 0,
                errors: vec![],
            });
        }

        let format_version = reader.read_u32::<LittleEndian>().unwrap_or(0);
        let _signature = reader.read_u32::<LittleEndian>().unwrap_or(0);
        let _unused = reader.read_u32::<LittleEndian>().unwrap_or(0);
        let file_size = reader.read_u32::<LittleEndian>().unwrap_or(0);

        let exe_name =
            Self::read_utf16le_string(&mut reader, 60).unwrap_or_else(|| "unknown".to_string());

        let hash = reader.read_u32::<LittleEndian>().unwrap_or(0);
        let _flags = reader.read_u32::<LittleEndian>().unwrap_or(0);

        let run_count = if format_version >= 30 {
            let _skip_count = 12;
            let mut skip = vec![0u8; _skip_count];
            reader.read_exact(&mut skip).ok();
            reader.read_u32::<LittleEndian>().unwrap_or(0)
        } else {
            reader.read_u32::<LittleEndian>().unwrap_or(0)
        };

        let mut run_times: Vec<DateTime<Utc>> = Vec::new();
        for _ in 0..8 {
            let ft = reader.read_u64::<LittleEndian>().unwrap_or(0);
            if let Some(dt) = Self::filetime_to_dt(ft) {
                if dt.year() > 2000 && dt.year() < 2100 {
                    run_times.push(dt);
                }
            }
        }

        let mut attrs = BTreeMap::new();
        attrs.insert("format_version".into(), format_version.into());
        attrs.insert("executable".into(), exe_name.clone().into());
        attrs.insert("run_count".into(), run_count.into());
        attrs.insert("hash".into(), format!("{:08X}", hash).into());
        attrs.insert("file_size".into(), file_size.into());
        let times_str: Vec<String> = run_times.iter().map(|t| t.to_rfc3339()).collect();
        attrs.insert(
            "last_run_times".into(),
            serde_json::Value::Array(times_str.iter().map(|s| s.clone().into()).collect()),
        );

        let artifact = new_artifact(
            "Prefetch",
            format!("Prefetch: {}", exe_name),
            format!(
                "{} executed {} times (fmt v{})",
                exe_name, run_count, format_version
            ),
            Some(&ctx.file_id),
            attrs,
        );
        sink.write_artifact(artifact);

        let mut events = 0u32;
        for rt in &run_times {
            let ev = new_timeline_event(
                &ctx.file_id,
                "PROGRAM_EXECUTION",
                *rt,
                format!("{} executed", exe_name),
                format!("Prefetch run at {}", rt.to_rfc3339()),
                BTreeMap::from([("executable".into(), exe_name.clone().into())]),
            );
            sink.write_timeline_event(ev);
            events += 1;
        }

        Ok(ExtractorReport {
            artifacts_found: 1,
            timeline_events: events,
            errors: vec![],
        })
    }
}
