//! Windows Shell Link (.lnk) parser.
//! Parses ShellLinkHeader + LinkInfo for target path extraction.
//! Does not yet parse StringData or ExtraData sections.

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

const HAS_LINK_TARGET_ID_LIST: u32 = 0x00000001;
const HAS_LINK_INFO: u32 = 0x00000002;

impl LnkExtractor {
    fn filetime_to_dt(ft: u64) -> Option<DateTime<Utc>> {
        if ft == 0 || ft >= 0x8000000000000000 {
            return None;
        }
        let secs = (ft / 10_000_000) as i64 - 11_644_473_600;
        Utc.timestamp_opt(secs, ((ft % 10_000_000) * 100) as u32)
            .single()
    }

    fn read_null_string(reader: &mut impl Read, max_bytes: usize) -> Option<String> {
        let mut buf = Vec::new();
        let mut b = [0u8; 1];
        for _ in 0..max_bytes {
            if reader.read_exact(&mut b).is_err() {
                break;
            }
            if b[0] == 0 {
                break;
            }
            buf.push(b[0]);
        }
        if buf.is_empty() {
            None
        } else {
            String::from_utf8(buf).ok()
        }
    }
}

impl ArtifactExtractor for LnkExtractor {
    fn id(&self) -> &'static str {
        "lnk"
    }
    fn display_name(&self) -> &'static str {
        "Windows LNK Shortcut Parser"
    }
    fn family(&self) -> ArtifactFamily {
        ArtifactFamily {
            name: "LNK".into(),
            description: Some("Windows Shell Link (.lnk)".into()),
        }
    }
    fn supports_path(&self, file_path: &str) -> bool {
        file_path.to_lowercase().ends_with(".lnk")
    }

    fn run(
        &self,
        ctx: ArtifactContext,
        sink: &mut dyn ArtifactSink,
    ) -> Result<ExtractorReport, String> {
        let mut reader = ctx.reader;

        // Header size
        let header_size = reader.read_u32::<LittleEndian>().unwrap_or(0x4C);
        if header_size < 0x4C {
            return Err("LNK header too small".to_string());
        }

        // Link CLSID
        let mut clsid = [0u8; 16];
        reader.read_exact(&mut clsid).map_err(|e| e.to_string())?;

        // LinkFlags
        let flags = reader.read_u32::<LittleEndian>().unwrap_or(0);

        // FileAttributes
        let _file_attrs = reader.read_u32::<LittleEndian>().unwrap_or(0);

        // Timestamps (FILETIME)
        let creation_time = reader.read_u64::<LittleEndian>().unwrap_or(0);
        let access_time = reader.read_u64::<LittleEndian>().unwrap_or(0);
        let write_time = reader.read_u64::<LittleEndian>().unwrap_or(0);

        // FileSize
        let file_size = reader.read_u32::<LittleEndian>().unwrap_or(0);

        // IconIndex + ShowCommand + HotKey
        let _icon_index = reader.read_i32::<LittleEndian>().unwrap_or(0);
        let _show_command = reader.read_u32::<LittleEndian>().unwrap_or(0);
        let mut _hotkey = [0u8; 2];
        reader.read_exact(&mut _hotkey).ok();
        let mut _reserved = [0u8; 10];
        reader.read_exact(&mut _reserved).ok();

        let mut attrs = BTreeMap::new();
        attrs.insert("file_size".into(), file_size.into());

        let mut timelines = 0u32;
        let mut add_time = |field: &str, event_type: &str, ft: u64| {
            if let Some(dt) = LnkExtractor::filetime_to_dt(ft) {
                if dt.year() > 2000 && dt.year() < 2100 {
                    attrs.insert(field.into(), dt.to_rfc3339().into());
                    let ev = new_timeline_event(
                        &ctx.file_id,
                        event_type,
                        dt,
                        format!("LNK time: {}", ctx.file_path),
                        format!("{} at {}", event_type, dt.to_rfc3339()),
                        BTreeMap::new(),
                    );
                    sink.write_timeline_event(ev);
                    return 1;
                }
            }
            0
        };
        timelines += add_time("creation_time", "LINK_CREATED", creation_time);
        timelines += add_time("access_time", "LINK_ACCESSED", access_time);
        timelines += add_time("write_time", "LINK_MODIFIED", write_time);

        // LinkTargetIDList
        let mut target_path = String::new();
        if flags & HAS_LINK_TARGET_ID_LIST != 0 {
            let id_list_size = reader.read_u16::<LittleEndian>().unwrap_or(0) as usize;
            if id_list_size > 2 {
                let mut skip = vec![0u8; id_list_size - 2];
                reader.read_exact(&mut skip).ok();
            }
        }

        // LinkInfo
        if flags & HAS_LINK_INFO != 0 {
            let link_info_size = reader.read_u32::<LittleEndian>().unwrap_or(0);
            if link_info_size >= 28 {
                let _link_info_flags = reader.read_u32::<LittleEndian>().unwrap_or(0);
                let _volume_id_offset = reader.read_u32::<LittleEndian>().unwrap_or(0);
                let local_base_path_offset = reader.read_u32::<LittleEndian>().unwrap_or(0);
                if local_base_path_offset >= 16
                    && (local_base_path_offset as u64) < link_info_size as u64
                {
                    // Skip bytes from current position (16) to local_base_path_offset
                    let skip = local_base_path_offset.saturating_sub(16) as usize;
                    if skip > 0 {
                        let mut _drain = vec![0u8; skip.min(256)];
                        reader.read_exact(&mut _drain).ok();
                    }
                    let remaining =
                        (link_info_size as usize).saturating_sub(local_base_path_offset as usize);
                    target_path =
                        Self::read_null_string(&mut reader, remaining.min(520)).unwrap_or_default();
                }
            }
        }

        if !target_path.is_empty() {
            attrs.insert("target_path".into(), target_path.clone().into());
        }

        let summary = if !target_path.is_empty() {
            format!("Shortcut → {}", target_path)
        } else {
            format!("Shell link, {} bytes", file_size)
        };

        let artifact = new_artifact(
            "LNK",
            format!("LNK: {}", ctx.file_path),
            summary,
            Some(&ctx.file_id),
            attrs,
        );
        sink.write_artifact(artifact);

        Ok(ExtractorReport {
            artifacts_found: 1,
            timeline_events: timelines,
            errors: vec![],
        })
    }
}
