//! Deprecated simplified Windows Registry hive reader.
//!
//! This base-block-only extractor is kept as a fallback for legacy tests.  The
//! canonical production path for registry extraction is the `lookup` module
//! together with `app_services::analysis_service::extraction::registry::
//! extract_registry_candidate`, which provides SYSTEM/SOFTWARE/SAM/NTUSER/
//! USRCLASS/Amcache/SECURITY field extraction, transaction-log merge, and
//! structured warnings.

use artifacts_core::{
    new_artifact, new_timeline_event, ArtifactContext, ArtifactExtractor, ArtifactSink,
    ExtractorReport,
};
use byteorder::{LittleEndian, ReadBytesExt};
use chrono::{DateTime, Datelike, TimeZone, Utc};
use domain::ArtifactFamily;
use std::collections::BTreeMap;
use std::io::{Read, Seek, SeekFrom};

pub struct RegistryExtractor;

struct RegistryHive {
    last_written: u64,
    root_cell_offset: u64,
}

impl RegistryExtractor {
    fn filetime_to_dt(ft: u64) -> Option<DateTime<Utc>> {
        if ft == 0 {
            return None;
        }
        let secs = (ft / 10_000_000) as i64 - 11_644_473_600;
        Utc.timestamp_opt(secs, ((ft % 10_000_000) * 100) as u32)
            .single()
    }

    fn parse_base_block(reader: &mut (impl Read + Seek)) -> Result<RegistryHive, String> {
        let mut magic = [0u8; 4];
        reader.read_exact(&mut magic).map_err(|e| e.to_string())?;
        if &magic != b"regf" {
            return Err("Not a valid registry hive".to_string());
        }

        let _seq1 = reader.read_u32::<LittleEndian>().unwrap_or(0);
        let _seq2 = reader.read_u32::<LittleEndian>().unwrap_or(0);
        let last_written = reader.read_u64::<LittleEndian>().unwrap_or(0);

        let _major = reader.read_u32::<LittleEndian>().unwrap_or(0);
        let _minor = reader.read_u32::<LittleEndian>().unwrap_or(0);

        let _file_type = reader.read_u32::<LittleEndian>().unwrap_or(0);
        let _format = reader.read_u32::<LittleEndian>().unwrap_or(0);
        let root_cell_offset = reader.read_u32::<LittleEndian>().unwrap_or(0) as u64;
        let _hbin_data_size = reader.read_u32::<LittleEndian>().unwrap_or(0);

        let mut _name_buf = [0u8; 64];
        reader
            .read_exact(&mut _name_buf)
            .map_err(|e| e.to_string())?;

        Ok(RegistryHive {
            last_written,
            root_cell_offset: root_cell_offset + 0x1000,
        })
    }

    fn read_hive_name(
        reader: &mut (impl Read + Seek),
        hive: &RegistryHive,
    ) -> Result<String, String> {
        reader
            .seek(SeekFrom::Start(hive.root_cell_offset))
            .map_err(|e| e.to_string())?;

        let _size = reader
            .read_i32::<LittleEndian>()
            .map_err(|e| e.to_string())?;
        let signature = {
            let mut sig = [0u8; 2];
            reader.read_exact(&mut sig).map_err(|e| e.to_string())?;
            sig
        };

        if signature != [b'n', b'k'] {
            return Ok("(non-nk root)".to_string());
        }

        let _flags = reader.read_u16::<LittleEndian>().unwrap_or(0);
        let _last_written = reader.read_u64::<LittleEndian>().unwrap_or(0);
        let _parent = reader.read_u32::<LittleEndian>().unwrap_or(0);
        let _subkeys_count = reader.read_u32::<LittleEndian>().unwrap_or(0);
        let _subkeys_list = reader.read_u32::<LittleEndian>().unwrap_or(0);
        let _values_list = reader.read_u32::<LittleEndian>().unwrap_or(0);
        let _security = reader.read_u32::<LittleEndian>().unwrap_or(0);
        let _classname = reader.read_u32::<LittleEndian>().unwrap_or(0);

        let _max_name_bytes = std::cmp::min(
            reader.read_u32::<LittleEndian>().unwrap_or(64) as usize,
            256,
        );
        let _max_class_bytes = reader.read_u32::<LittleEndian>().unwrap_or(0) as usize;
        let _mod_subkeys = reader.read_u32::<LittleEndian>().unwrap_or(0);
        let _mod_values = reader.read_u32::<LittleEndian>().unwrap_or(0);
        let name_len = reader.read_u16::<LittleEndian>().unwrap_or(0) as usize;

        let name_len = name_len.min(128);
        let mut name_bytes = vec![0u8; name_len];
        reader
            .read_exact(&mut name_bytes)
            .map_err(|e| e.to_string())?;

        if name_bytes.len() >= 2 {
            let chars: Vec<u16> = name_bytes
                .chunks_exact(2)
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .collect();
            String::from_utf16(&chars).map_err(|e| e.to_string())
        } else {
            Ok(String::new())
        }
    }
}

impl ArtifactExtractor for RegistryExtractor {
    fn id(&self) -> &'static str {
        "registry"
    }
    fn display_name(&self) -> &'static str {
        "Windows Registry Hive Parser"
    }
    fn family(&self) -> ArtifactFamily {
        ArtifactFamily {
            name: "Registry".into(),
            description: Some("Windows Registry hives".into()),
        }
    }
    fn supports_path(&self, file_path: &str) -> bool {
        let normalized = file_path.replace('\\', "/").to_ascii_lowercase();
        let name = normalized.rsplit('/').next().unwrap_or(&normalized);
        if name.ends_with(".dat") {
            return matches!(
                name,
                "ntuser.dat"
                    | "usrclass.dat"
                    | "system.dat"
                    | "software.dat"
                    | "sam.dat"
                    | "security.dat"
            );
        }
        let components: Vec<_> = normalized
            .split('/')
            .filter(|part| !part.is_empty())
            .collect();
        if components.len() < 4 {
            return false;
        }
        let expected = ["windows", "system32", "config"];
        let last_four = &components[components.len() - 4..];
        last_four[..3] == expected
            && matches!(
                last_four[3],
                "system" | "software" | "sam" | "security" | "default"
            )
    }

    fn run(
        &self,
        ctx: ArtifactContext,
        sink: &mut dyn ArtifactSink,
    ) -> Result<ExtractorReport, String> {
        let mut buf = Vec::new();
        ctx.reader
            .take(20 * 1024 * 1024)
            .read_to_end(&mut buf)
            .map_err(|e| e.to_string())?;
        let mut reader = std::io::BufReader::new(std::io::Cursor::new(buf));
        let hive = Self::parse_base_block(&mut reader)?;
        let name =
            Self::read_hive_name(&mut reader, &hive).unwrap_or_else(|_| "unknown".to_string());

        let mut attrs = BTreeMap::new();
        attrs.insert("hive_name".into(), serde_json::Value::String(name.clone()));

        let mut timeline_events = 0;
        if let Some(dt) = Self::filetime_to_dt(hive.last_written) {
            if dt.year() > 2000 {
                attrs.insert(
                    "last_written".into(),
                    serde_json::Value::String(dt.to_rfc3339()),
                );
                let ev = new_timeline_event(
                    &ctx.file_id,
                    "REGISTRY_MODIFIED",
                    dt,
                    format!("Registry hive modified: {}", name),
                    format!("Hive last written at {}", dt.to_rfc3339()),
                    BTreeMap::new(),
                );
                sink.write_timeline_event(ev);
                timeline_events += 1;
            }
        }

        let artifact = new_artifact(
            "Registry",
            format!("Registry Hive: {}", name),
            format!("Windows registry hive '{}'", name),
            Some(&ctx.file_id),
            attrs,
        );
        sink.write_artifact(artifact);

        Ok(ExtractorReport {
            artifacts_found: 1,
            timeline_events,
            errors: vec![],
        })
    }
}
