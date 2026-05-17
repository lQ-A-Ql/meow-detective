//! Windows Shell Link (.lnk) parser.
//! Parses the ShellLinkHeader (76 bytes) to extract timestamps and file size.
//! Does not yet parse LinkTargetIDList or StringData sections.

use artifacts_core::{
    new_artifact, new_timeline_event, ArtifactContext, ArtifactExtractor, ArtifactSink,
    ExtractorReport,
};
use byteorder::{LittleEndian, ReadBytesExt};
use chrono::{DateTime, Datelike, TimeZone, Utc};
use domain::ArtifactFamily;
use std::collections::BTreeMap;
use std::io::Read;

pub struct LnkExtractor;

struct ShellLinkHeader {
    _guid: [u8; 16],
    _flags: u32,
    _file_attributes: u32,
    creation_time: u64,
    access_time: u64,
    write_time: u64,
    file_size: u32,
    _icon_index: i32,
    show_command: u32,
    _hotkey: [u8; 2],
}

impl ShellLinkHeader {
    fn parse<R: Read>(reader: &mut R) -> Result<Self, String> {
        let mut magic = [0u8; 4];
        reader.read_exact(&mut magic).map_err(|e| e.to_string())?;
        if &magic != b"L\x00\x00\x00" {
            return Err("Not a valid LNK file".to_string());
        }
        let mut guid = [0u8; 16];
        reader.read_exact(&mut guid).map_err(|e| e.to_string())?;
        let flags = reader.read_u32::<LittleEndian>().unwrap_or(0);
        let file_attributes = reader.read_u32::<LittleEndian>().unwrap_or(0);
        let creation_time = reader.read_u64::<LittleEndian>().unwrap_or(0);
        let access_time = reader.read_u64::<LittleEndian>().unwrap_or(0);
        let write_time = reader.read_u64::<LittleEndian>().unwrap_or(0);
        let file_size = reader.read_u32::<LittleEndian>().unwrap_or(0);
        let icon_index = reader.read_i32::<LittleEndian>().unwrap_or(0);
        let show_command = reader.read_u32::<LittleEndian>().unwrap_or(0);
        let mut hotkey = [0u8; 2];
        reader.read_exact(&mut hotkey).map_err(|e| e.to_string())?;
        let _reserved = &mut [0u8; 10];
        reader.read_exact(_reserved).map_err(|e| e.to_string())?;
        Ok(Self { _guid: guid, _flags: flags, _file_attributes: file_attributes, creation_time, access_time, write_time, file_size, _icon_index: icon_index, show_command, _hotkey: hotkey })
    }
}

impl LnkExtractor {
    fn filetime_to_dt(ft: u64) -> Option<DateTime<Utc>> {
        if ft == 0 || ft >= 0x8000000000000000 { return None; }
        let secs = (ft / 10_000_000) as i64 - 11_644_473_600;
        Utc.timestamp_opt(secs, ((ft % 10_000_000) * 100) as u32).single()
    }
}

impl ArtifactExtractor for LnkExtractor {
    fn id(&self) -> &'static str { "lnk" }
    fn display_name(&self) -> &'static str { "Windows LNK Shortcut Parser" }
    fn family(&self) -> ArtifactFamily {
        ArtifactFamily { name: "LNK".into(), description: Some("Windows Shell Link (.lnk)".into()) }
    }
    fn supports_path(&self, file_path: &str) -> bool {
        file_path.to_lowercase().ends_with(".lnk")
    }

    fn run(&self, ctx: ArtifactContext, sink: &mut dyn ArtifactSink) -> Result<ExtractorReport, String> {
        let mut reader = ctx.reader;
        let header = ShellLinkHeader::parse(&mut reader)?;

        let mut attrs = BTreeMap::new();
        attrs.insert("file_size".into(), serde_json::Value::Number(header.file_size.into()));
        attrs.insert("show_command".into(), serde_json::Value::Number(header.show_command.into()));

        let mut timelines = 0u32;
        if let Some(dt) = Self::filetime_to_dt(header.creation_time) {
            if dt.year() > 2000 {
                attrs.insert("creation_time".into(), serde_json::Value::String(dt.to_rfc3339()));
                let ev = new_timeline_event(&ctx.file_id, "LINK_CREATED", dt,
                    format!("LNK target created: {}", ctx.file_path),
                    format!("LNK creation time {}", dt.to_rfc3339()), BTreeMap::new());
                sink.write_timeline_event(ev);
                timelines += 1;
            }
        }
        if let Some(dt) = Self::filetime_to_dt(header.write_time) {
            if dt.year() > 2000 {
                attrs.insert("write_time".into(), serde_json::Value::String(dt.to_rfc3339()));
                let ev = new_timeline_event(&ctx.file_id, "LINK_MODIFIED", dt,
                    format!("LNK target modified: {}", ctx.file_path),
                    format!("LNK write time {}", dt.to_rfc3339()), BTreeMap::new());
                sink.write_timeline_event(ev);
                timelines += 1;
            }
        }
        if let Some(dt) = Self::filetime_to_dt(header.access_time) {
            if dt.year() > 2000 {
                attrs.insert("access_time".into(), serde_json::Value::String(dt.to_rfc3339()));
                let ev = new_timeline_event(&ctx.file_id, "LINK_ACCESSED", dt,
                    format!("LNK target accessed: {}", ctx.file_path),
                    format!("LNK access time {}", dt.to_rfc3339()), BTreeMap::new());
                sink.write_timeline_event(ev);
                timelines += 1;
            }
        }

        let artifact = new_artifact("LNK", format!("LNK: {}", ctx.file_path),
            format!("Shell link, target size: {} bytes", header.file_size),
            Some(&ctx.file_id), attrs);
        sink.write_artifact(artifact);

        Ok(ExtractorReport { artifacts_found: 1, timeline_events: timelines, errors: vec![] })
    }
}
