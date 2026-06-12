//! Windows Prefetch parser.
//! Format reference: libscca / Windows Internals.
//! Supports uncompressed SCCA payloads and Windows 10+ MAM-compressed payloads.

use artifacts_core::{
    new_artifact, new_timeline_event, ArtifactContext, ArtifactExtractor, ArtifactSink,
    ExtractorReport,
};
use byteorder::{LittleEndian, ReadBytesExt};
use chrono::{DateTime, Datelike, TimeZone, Utc};
use domain::ArtifactFamily;
use std::collections::BTreeMap;
use std::io::{Cursor, Read};

const PREFETCH_UNCOMPRESSED_SIGNATURE: &[u8; 4] = b"SCCA";
const PREFETCH_MAM_SIGNATURE: &[u8; 4] = b"MAM\x04";
const PREFETCH_HEADER_SIZE: usize = 84;
const PREFETCH_V17_FILE_INFO_SIZE: usize = 68;
const PREFETCH_V23_FILE_INFO_SIZE: usize = 156;
const PREFETCH_V26_FILE_INFO_SIZE: usize = 220;
const PREFETCH_V30_VARIANT1_FILE_INFO_SIZE: usize = 220;
const PREFETCH_V30_VARIANT2_FILE_INFO_SIZE: usize = 212;
const PREFETCH_V31_FILE_INFO_SIZE: usize = 212;

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
        let end = buf
            .chunks_exact(2)
            .position(|chunk| chunk == [0, 0])
            .map(|idx| idx * 2)
            .unwrap_or(buf.len());
        let chars: Vec<u16> = buf[..end]
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        String::from_utf16(&chars).ok()
    }

    fn decode_prefetch_payload(data: Vec<u8>) -> Result<Vec<u8>, String> {
        if data.get(4..8) == Some(PREFETCH_UNCOMPRESSED_SIGNATURE.as_slice()) {
            return Ok(data);
        }
        if data.starts_with(PREFETCH_MAM_SIGNATURE) {
            return Self::decode_mam_prefetch(&data);
        }
        Err("Not a Prefetch file".to_string())
    }

    fn file_info_layout(format_version: u32, data: &[u8]) -> Result<(usize, usize), String> {
        match format_version {
            17 => Ok((PREFETCH_HEADER_SIZE, PREFETCH_V17_FILE_INFO_SIZE)),
            23 => Ok((PREFETCH_HEADER_SIZE, PREFETCH_V23_FILE_INFO_SIZE)),
            26 => Ok((PREFETCH_HEADER_SIZE, PREFETCH_V26_FILE_INFO_SIZE)),
            30 => {
                let header_and_variant2 =
                    PREFETCH_HEADER_SIZE + PREFETCH_V30_VARIANT2_FILE_INFO_SIZE;
                if data.len() >= header_and_variant2 {
                    let run_count_variant2 =
                        read_u32_le(data, PREFETCH_HEADER_SIZE + 116).unwrap_or(0);
                    let run_count_variant1 =
                        read_u32_le(data, PREFETCH_HEADER_SIZE + 124).unwrap_or(0);
                    let variant2_hash_offset =
                        read_u32_le(data, PREFETCH_HEADER_SIZE + 128).unwrap_or(0);
                    let variant1_hash_offset =
                        read_u32_le(data, PREFETCH_HEADER_SIZE + 136).unwrap_or(0);

                    let variant2_looks_valid =
                        variant2_hash_offset <= data.len() as u32 && run_count_variant2 > 0;
                    let variant1_looks_valid =
                        variant1_hash_offset <= data.len() as u32 && run_count_variant1 > 0;

                    if variant2_looks_valid || !variant1_looks_valid {
                        return Ok((PREFETCH_HEADER_SIZE, PREFETCH_V30_VARIANT2_FILE_INFO_SIZE));
                    }
                }
                Ok((PREFETCH_HEADER_SIZE, PREFETCH_V30_VARIANT1_FILE_INFO_SIZE))
            }
            31 => Ok((PREFETCH_HEADER_SIZE, PREFETCH_V31_FILE_INFO_SIZE)),
            other => Err(format!("Unsupported Prefetch format version: {}", other)),
        }
    }

    fn read_run_count(format_version: u32, file_info: &[u8]) -> u32 {
        match format_version {
            17 => read_u32_le(file_info, 60).unwrap_or(0),
            23 => read_u32_le(file_info, 68).unwrap_or(0),
            26 => read_u32_le(file_info, 124).unwrap_or(0),
            30 | 31 => {
                if file_info.len() >= PREFETCH_V30_VARIANT1_FILE_INFO_SIZE {
                    read_u32_le(file_info, 124).unwrap_or(0)
                } else {
                    read_u32_le(file_info, 116).unwrap_or(0)
                }
            }
            _ => 0,
        }
    }

    fn read_run_times(format_version: u32, file_info: &[u8]) -> Vec<DateTime<Utc>> {
        let (offset, slots) = match format_version {
            17 => (36usize, 1usize),
            23 => (44usize, 1usize),
            26 | 30 | 31 => (44usize, 8usize),
            _ => (0usize, 0usize),
        };

        let mut run_times = Vec::new();
        for idx in 0..slots {
            let ft = read_u64_le(file_info, offset + idx * 8).unwrap_or(0);
            if let Some(dt) = Self::filetime_to_dt(ft) {
                if dt.year() > 2000 && dt.year() < 2100 {
                    run_times.push(dt);
                }
            }
        }
        run_times
    }

    #[cfg(windows)]
    fn decode_mam_prefetch(data: &[u8]) -> Result<Vec<u8>, String> {
        use windows_sys::Win32::Storage::Compression::{
            COMPRESS_ALGORITHM_LZMS, COMPRESS_ALGORITHM_XPRESS, COMPRESS_ALGORITHM_XPRESS_HUFF,
            COMPRESS_RAW,
        };

        if data.len() < 10 {
            return Err("Compressed Prefetch payload is truncated".to_string());
        }

        let expected_size = u32::from_le_bytes([data[4], data[5], data[6], data[7]]) as usize;
        if expected_size == 0 || expected_size > 128 * 1024 * 1024 {
            return Err("Compressed Prefetch declares an invalid uncompressed size".to_string());
        }

        let compressed = &data[8..];
        let algorithms = [
            COMPRESS_ALGORITHM_XPRESS_HUFF | COMPRESS_RAW,
            COMPRESS_ALGORITHM_XPRESS | COMPRESS_RAW,
            COMPRESS_ALGORITHM_LZMS | COMPRESS_RAW,
            COMPRESS_ALGORITHM_XPRESS_HUFF,
            COMPRESS_ALGORITHM_XPRESS,
            COMPRESS_ALGORITHM_LZMS,
        ];

        let mut last_error = None;
        for algorithm in algorithms {
            match try_decompress_with_windows_api(algorithm, compressed, expected_size) {
                Ok(decoded)
                    if decoded.get(4..8) == Some(PREFETCH_UNCOMPRESSED_SIGNATURE.as_slice()) =>
                {
                    return Ok(decoded);
                }
                Ok(_) => {
                    last_error = Some(
                        "Compressed Prefetch decompressed but did not yield an SCCA payload"
                            .to_string(),
                    );
                }
                Err(err) => last_error = Some(err),
            }
        }

        Err(last_error.unwrap_or_else(|| {
            "Compressed Prefetch could not be decompressed with supported algorithms".to_string()
        }))
    }

    #[cfg(not(windows))]
    fn decode_mam_prefetch(_data: &[u8]) -> Result<Vec<u8>, String> {
        Err("MAM-compressed Prefetch requires Windows decompression support".to_string())
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
        mut ctx: ArtifactContext,
        sink: &mut dyn ArtifactSink,
    ) -> Result<ExtractorReport, String> {
        let mut raw_data = Vec::new();
        ctx.reader
            .read_to_end(&mut raw_data)
            .map_err(|e| e.to_string())?;
        let data = match Self::decode_prefetch_payload(raw_data) {
            Ok(data) => data,
            Err(err) if err == "Not a Prefetch file" => {
                return Ok(ExtractorReport {
                    artifacts_found: 0,
                    timeline_events: 0,
                    errors: vec![],
                });
            }
            Err(err) => {
                return Ok(ExtractorReport {
                    artifacts_found: 0,
                    timeline_events: 0,
                    errors: vec![err],
                });
            }
        };
        if data.len() < PREFETCH_HEADER_SIZE {
            return Ok(ExtractorReport {
                artifacts_found: 0,
                timeline_events: 0,
                errors: vec!["Prefetch payload is truncated".to_string()],
            });
        }

        let mut reader = Cursor::new(&data);
        let format_version = reader.read_u32::<LittleEndian>().unwrap_or(0);
        let mut signature = [0u8; 4];
        reader
            .read_exact(&mut signature)
            .map_err(|e| e.to_string())?;
        if &signature != PREFETCH_UNCOMPRESSED_SIGNATURE {
            return Ok(ExtractorReport {
                artifacts_found: 0,
                timeline_events: 0,
                errors: vec!["Not a Prefetch file".to_string()],
            });
        }

        let _unknown = reader.read_u32::<LittleEndian>().unwrap_or(0);
        let file_size = reader.read_u32::<LittleEndian>().unwrap_or(0);
        let exe_name =
            Self::read_utf16le_string(&mut reader, 60).unwrap_or_else(|| "unknown".to_string());
        let hash = reader.read_u32::<LittleEndian>().unwrap_or(0);
        let _flags = reader.read_u32::<LittleEndian>().unwrap_or(0);

        let (file_info_offset, file_info_size) = match Self::file_info_layout(format_version, &data)
        {
            Ok(layout) => layout,
            Err(err) => {
                return Ok(ExtractorReport {
                    artifacts_found: 0,
                    timeline_events: 0,
                    errors: vec![err],
                });
            }
        };
        if data.len() < file_info_offset + file_info_size {
            return Ok(ExtractorReport {
                artifacts_found: 0,
                timeline_events: 0,
                errors: vec!["Prefetch file information section is truncated".to_string()],
            });
        }
        let file_info = &data[file_info_offset..file_info_offset + file_info_size];

        let run_count = Self::read_run_count(format_version, file_info);
        let run_times = Self::read_run_times(format_version, file_info);

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

fn read_u32_le(data: &[u8], offset: usize) -> Option<u32> {
    let bytes = data.get(offset..offset + 4)?;
    Some(u32::from_le_bytes(bytes.try_into().ok()?))
}

fn read_u64_le(data: &[u8], offset: usize) -> Option<u64> {
    let bytes = data.get(offset..offset + 8)?;
    Some(u64::from_le_bytes(bytes.try_into().ok()?))
}

#[cfg(windows)]
fn try_decompress_with_windows_api(
    algorithm: u32,
    compressed: &[u8],
    expected_size: usize,
) -> Result<Vec<u8>, String> {
    use windows_sys::Win32::Storage::Compression::{
        CloseDecompressor, CreateDecompressor, Decompress, DECOMPRESSOR_HANDLE,
    };

    let mut handle: DECOMPRESSOR_HANDLE = std::ptr::null_mut();
    // SAFETY: Windows Compression API expects a valid output pointer for the handle.
    let created = unsafe { CreateDecompressor(algorithm, std::ptr::null(), &mut handle) };
    if created == 0 || handle.is_null() {
        return Err(last_os_error_message("CreateDecompressor failed"));
    }

    struct DecompressorGuard(DECOMPRESSOR_HANDLE);
    impl Drop for DecompressorGuard {
        fn drop(&mut self) {
            if !self.0.is_null() {
                // SAFETY: handle was returned by CreateDecompressor and is closed once here.
                unsafe {
                    CloseDecompressor(self.0);
                }
            }
        }
    }
    let _guard = DecompressorGuard(handle);

    let mut decoded = vec![0u8; expected_size];
    let mut actual_size = 0usize;

    // SAFETY: compressed/decoded buffers are valid for the specified sizes.
    let ok = unsafe {
        Decompress(
            handle,
            compressed.as_ptr().cast(),
            compressed.len(),
            decoded.as_mut_ptr().cast(),
            decoded.len(),
            &mut actual_size,
        )
    };

    if ok == 0 {
        return Err(last_os_error_message("Decompress failed"));
    }
    decoded.truncate(actual_size);
    Ok(decoded)
}

#[cfg(windows)]
fn last_os_error_message(prefix: &str) -> String {
    let error = std::io::Error::last_os_error();
    format!("{}: {}", prefix, error)
}

#[cfg(test)]
mod tests {
    use super::*;
    use artifacts_core::VecSink;
    use domain::FileEntryId;

    #[test]
    fn mam_prefetch_without_payload_fails_closed() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"MAM\x04");
        bytes.extend_from_slice(&4096u32.to_le_bytes());
        bytes.resize(128, 0);

        let ctx = ArtifactContext {
            file_id: FileEntryId("pf-1".to_string()),
            file_path: "C:/Windows/Prefetch/CMD.EXE-1234.pf".to_string(),
            reader: Box::new(std::io::Cursor::new(bytes)),
        };
        let mut sink = VecSink::new();

        let report = PrefetchExtractor.run(ctx, &mut sink).unwrap();

        assert_eq!(report.artifacts_found, 0);
        assert_eq!(report.timeline_events, 0);
        assert_eq!(report.errors.len(), 1);
        assert!(sink.artifacts.is_empty());
    }
}
