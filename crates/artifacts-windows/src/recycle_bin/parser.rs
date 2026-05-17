use artifacts_core::{
    new_artifact, new_timeline_event, ArtifactContext, ArtifactExtractor, ArtifactSink,
    ExtractorReport,
};
use byteorder::{LittleEndian, ReadBytesExt};
use chrono::{DateTime, Datelike, TimeZone, Utc};
use domain::ArtifactFamily;
use std::collections::BTreeMap;
use std::io::Read;

pub struct RecycleBinExtractor;

impl RecycleBinExtractor {
    fn filetime_to_dt(ft: u64) -> Option<DateTime<Utc>> {
        if ft == 0 || ft >= 0x8000000000000000 { return None; }
        let secs = (ft / 10_000_000) as i64 - 11_644_473_600;
        Utc.timestamp_opt(secs, ((ft % 10_000_000) * 100) as u32).single()
    }

    fn parse_i_file<R: Read>(reader: &mut R) -> Result<(u64, Option<DateTime<Utc>>, Option<String>), String> {
        let header_size = reader.read_u64::<LittleEndian>().map_err(|e| e.to_string())?;
        let file_size = reader.read_u64::<LittleEndian>().map_err(|e| e.to_string())?;
        let deletion_ft = reader.read_u64::<LittleEndian>().map_err(|e| e.to_string())?;
        let deletion_time = Self::filetime_to_dt(deletion_ft);

        let name_bytes_remaining = if header_size >= 28 {
            header_size.saturating_sub(28) as usize
        } else {
            0
        };

        let path = if name_bytes_remaining > 0 {
            let mut raw = vec![0u8; name_bytes_remaining.min(520)];
            let n = reader.read(&mut raw).map_err(|e| e.to_string())?;
            raw.truncate(n);
            let chars: Vec<u16> = raw.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
            let s = String::from_utf16(&chars).unwrap_or_default();
            let s = s.trim_end_matches('\0').to_string();
            if s.is_empty() { None } else { Some(s) }
        } else {
            None
        };

        Ok((file_size, deletion_time, path))
    }
}

impl ArtifactExtractor for RecycleBinExtractor {
    fn id(&self) -> &'static str { "recycle_bin" }
    fn display_name(&self) -> &'static str { "Windows Recycle Bin Parser" }
    fn family(&self) -> ArtifactFamily {
        ArtifactFamily { name: "RecycleBin".into(), description: Some("Windows Recycle Bin ($I/$R)".into()) }
    }
    fn supports(&self, ctx: &ArtifactContext) -> bool {
        ctx.file_path.contains("$Recycle.Bin") && ctx.file_path.contains("$I")
    }

    fn run(&self, ctx: ArtifactContext, sink: &mut dyn ArtifactSink) -> Result<ExtractorReport, String> {
        let mut reader = ctx.reader;
        let (file_size, deletion_time, original_path) = Self::parse_i_file(&mut reader)?;

        let mut attrs = BTreeMap::new();
        attrs.insert("recovered_file_size".into(), serde_json::Value::Number(file_size.into()));
        if let Some(ref path) = original_path {
            attrs.insert("original_path".into(), serde_json::Value::String(path.clone()));
        }

        let summary = match (&original_path, deletion_time) {
            (Some(p), Some(t)) => format!("Deleted: {} at {}", p, t.to_rfc3339()),
            (Some(p), None) => format!("Deleted: {}", p),
            (None, _) => format!("Recycled file, {} bytes", file_size),
        };

        let artifact = new_artifact("RecycleBin", format!("Recycle Bin: deleted file"),
            summary, Some(&ctx.file_id), attrs);
        sink.write_artifact(artifact);

        let mut events = 0u32;
        if let Some(dt) = deletion_time {
            if dt.year() > 2000 {
                let mut tl_attrs = BTreeMap::new();
                if let Some(ref p) = original_path { tl_attrs.insert("path".into(), serde_json::Value::String(p.clone())); }
                let ev = new_timeline_event(&ctx.file_id, "FILE_DELETED", dt,
                    "File deleted".into(),
                    format!("File deleted at {}", dt.to_rfc3339()), tl_attrs);
                sink.write_timeline_event(ev);
                events += 1;
            }
        }

        Ok(ExtractorReport { artifacts_found: 1, timeline_events: events, errors: vec![] })
    }
}
