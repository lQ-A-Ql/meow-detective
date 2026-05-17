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
        let nanos = ((ft % 10_000_000) * 100) as u32;
        Utc.timestamp_opt(secs, nanos).single()
    }

    fn read_utf16le_null(reader: &mut impl Read, max_bytes: usize) -> Option<String> {
        let mut buf = Vec::new();
        let mut pair = [0u8; 2];
        for _ in 0..max_bytes / 2 {
            if reader.read_exact(&mut pair).is_err() { break; }
            if pair[0] == 0 && pair[1] == 0 { break; }
            buf.extend_from_slice(&pair);
        }
        if buf.is_empty() { None } else { String::from_utf16(&u16_chunks(&buf)).ok() }
    }
}

impl ArtifactExtractor for PrefetchExtractor {
    fn id(&self) -> &'static str { "prefetch" }
    fn display_name(&self) -> &'static str { "Windows Prefetch Parser" }

    fn family(&self) -> ArtifactFamily {
        ArtifactFamily { name: "Prefetch".into(), description: Some("Windows Prefetch files (.pf)".into()) }
    }

    fn supports(&self, ctx: &ArtifactContext) -> bool {
        ctx.file_path.to_lowercase().ends_with(".pf")
    }

    fn run(&self, ctx: ArtifactContext, sink: &mut dyn ArtifactSink) -> Result<ExtractorReport, String> {
        let mut reader = ctx.reader;
        let mut magic = [0u8; 4];
        reader.read_exact(&mut magic).map_err(|e| e.to_string())?;

        if &magic != b"SCCA" && &magic != b"MAM\x04" {
            return Ok(ExtractorReport { artifacts_found: 0, timeline_events: 0, errors: vec!["Invalid Prefetch magic".into()] });
        }

        let _format_version = reader.read_u32::<LittleEndian>().unwrap_or(0);

        let exe_name = Self::read_utf16le_null(&mut reader, 120)
            .unwrap_or_else(|| "unknown".to_string());

        let run_count = reader.read_u32::<LittleEndian>().unwrap_or(0);

        let _skip = &mut [0u8; 4];
        reader.read_exact(_skip).ok();

        let mut run_times: Vec<DateTime<Utc>> = Vec::new();
        for _ in 0..8 {
            let ft = reader.read_u64::<LittleEndian>().unwrap_or(0);
            if let Some(dt) = Self::filetime_to_dt(ft) {
                if dt.year() > 2000 {
                    run_times.push(dt);
                }
            }
        }

        let mut attrs = BTreeMap::new();
        attrs.insert("executable".into(), serde_json::Value::String(exe_name.clone()));
        attrs.insert("run_count".into(), serde_json::Value::Number(run_count.into()));
        let run_times_str: Vec<String> = run_times.iter().map(|t| t.to_rfc3339()).collect();
        attrs.insert("last_run_times".into(), serde_json::Value::Array(
            run_times_str.iter().map(|s| serde_json::Value::String(s.clone())).collect()
        ));

        let artifact = new_artifact(
            "Prefetch",
            format!("Prefetch: {}", exe_name),
            format!("{} executed {} times", exe_name, run_count),
            Some(&ctx.file_id),
            attrs,
        );
        sink.write_artifact(artifact);

        let mut events = 0u32;
        for rt in &run_times {
            let mut tl_attrs = BTreeMap::new();
            tl_attrs.insert("executable".into(), serde_json::Value::String(exe_name.clone()));
            let event = new_timeline_event(
                &ctx.file_id,
                "PROGRAM_EXECUTION",
                *rt,
                format!("{} executed", exe_name),
                format!("Prefetch run at {}", rt.to_rfc3339()),
                tl_attrs,
            );
            sink.write_timeline_event(event);
            events += 1;
        }

        Ok(ExtractorReport { artifacts_found: 1, timeline_events: events, errors: vec![] })
    }
}

fn u16_chunks(data: &[u8]) -> Vec<u16> {
    data.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect()
}
